//! TUI application state machine

use crate::client::Client;
use crate::error::TuiError;
use ccm_config::{Config, KeybindMap};
use ccm_proto::daemon::{
    event as daemon_event, AttachInput, Event as DaemonEvent, LineCommentInfo, RepoInfo,
    SessionInfo, TodoItem, WorktreeInfo,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, size, BeginSynchronizedUpdate, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{debug, warn};

type Result<T> = std::result::Result<T, TuiError>;

use super::input::{handle_input_sync, handle_mouse_sync, TextInput};
use super::layout::draw;
use super::state::{
    AsyncAction, DeleteTarget, DiffItem, DirtyFlags, ExitCleanupAction, Focus, GitPanelItem,
    GitSection, GitStatusFile, InputMode, PrefixMode, RepoState, RightPanelView, SidebarItem,
    SidebarState, TerminalMode, TerminalState, TodoState,
};
use super::widgets::VirtualList;
use std::collections::HashMap;

/// Terminal stream state for a session
pub struct TerminalStream {
    pub session_id: String,
    pub input_tx: mpsc::Sender<AttachInput>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,
}

/// Deactivate fcitx5 input method
fn deactivate_ime() {
    let _ = std::process::Command::new("fcitx5-remote")
        .arg("-c")
        .output();
}

/// Activate fcitx5 input method
fn activate_ime() {
    let _ = std::process::Command::new("fcitx5-remote")
        .arg("-o")
        .output();
}

/// Application state
pub struct App {
    pub client: Client,

    // ============ Repo Management ============
    /// Per-repo state keyed by repo_id (contains all repo-specific data)
    pub repo_states: HashMap<String, RepoState>,
    /// Order of repos for display (list of repo_ids)
    pub repo_order: Vec<String>,
    /// Currently selected repo ID
    pub current_repo_id: Option<String>,

    // ============ Global UI State ============
    /// Focus position
    pub focus: Focus,
    /// Focus restoration stack for popups/dialogs
    pub saved_focus_stack: Vec<Focus>,

    // ============ Terminal State (global, shared across repos) ============
    pub terminal: TerminalState,
    pub terminal_stream: Option<TerminalStream>,

    // ============ Sidebar State (global parts only) ============
    pub sidebar: SidebarState,

    // ============ TODO State (global) ============
    pub todo: TodoState,

    // ============ View State ============
    /// Right panel view mode (shared between terminal and diff)
    pub right_panel_view: RightPanelView,

    // ============ UI State ============
    pub should_quit: bool,
    pub error_message: Option<String>,
    pub status_message: Option<String>,
    pub input_mode: InputMode,
    pub text_input: TextInput,
    pub session_delete_action: ExitCleanupAction,

    // ============ Event Subscription ============
    pub event_rx: Option<mpsc::Receiver<DaemonEvent>>,

    // ============ Git Refresh Debounce ============
    pub last_git_refresh: Option<std::time::Instant>,

    // ============ Prefix Key Mode ============
    pub prefix_mode: PrefixMode,

    // ============ Configuration ============
    #[allow(dead_code)]
    pub config: Config,
    pub keybinds: KeybindMap,

    // ============ Dirty Flags ============
    pub dirty: DirtyFlags,
}

impl App {
    pub async fn new(client: Client) -> Result<Self> {
        // Load configuration and build keybind map
        let config = Config::load_or_default()
            .map_err(|e| TuiError::Config(format!("Failed to load config: {}", e)))?;
        let keybinds = config
            .to_keybind_map()
            .map_err(|e| TuiError::Config(format!("Failed to build keybind map: {}", e)))?;

        let mut app = Self {
            client,
            // Repo management
            repo_states: HashMap::new(),
            repo_order: Vec::new(),
            current_repo_id: None,
            // Global UI
            focus: Focus::Sidebar,
            saved_focus_stack: Vec::new(),
            // Terminal
            terminal: TerminalState::default(),
            terminal_stream: None,
            // Sidebar (global parts)
            sidebar: SidebarState::default(),
            // TODO
            todo: TodoState::new(),
            // View
            right_panel_view: RightPanelView::Terminal,
            // UI state
            should_quit: false,
            error_message: None,
            status_message: None,
            input_mode: InputMode::Normal,
            text_input: TextInput::new(),
            session_delete_action: ExitCleanupAction::Destroy,
            // Event subscription
            event_rx: None,
            // Debounce
            last_git_refresh: None,
            // Prefix mode
            prefix_mode: PrefixMode::None,
            // Config
            config,
            keybinds,
            // Dirty flags
            dirty: DirtyFlags::default(),
        };

        // Load initial data
        app.refresh_all().await?;

        // Load git status for current worktree
        let _ = app.load_git_status().await;

        // Subscribe to events (don't fail if subscription fails)
        app.subscribe_events().await;

        Ok(app)
    }

    // ============ Repo Access Helpers ============

    /// Get current repo state (immutable)
    pub fn current_repo(&self) -> Option<&RepoState> {
        self.current_repo_id
            .as_ref()
            .and_then(|id| self.repo_states.get(id))
    }

    /// Get current repo state (mutable)
    pub fn current_repo_mut(&mut self) -> Option<&mut RepoState> {
        // Need to clone the id to avoid borrow issues
        let id = self.current_repo_id.clone()?;
        self.repo_states.get_mut(&id)
    }

    /// Get current repo index in repo_order
    pub fn repo_idx(&self) -> usize {
        self.current_repo_id
            .as_ref()
            .and_then(|id| self.repo_order.iter().position(|r| r == id))
            .unwrap_or(0)
    }

    /// Get repos in display order
    pub fn repos_ordered(&self) -> impl Iterator<Item = &RepoState> {
        self.repo_order
            .iter()
            .filter_map(|id| self.repo_states.get(id))
    }

    /// Get repos as RepoInfo list (for compatibility)
    pub fn repos(&self) -> Vec<RepoInfo> {
        self.repos_ordered().map(|r| r.info.clone()).collect()
    }

    /// Get current worktrees (convenience)
    pub fn worktrees(&self) -> &[WorktreeInfo] {
        self.current_repo()
            .map(|r| r.worktrees.as_slice())
            .unwrap_or(&[])
    }

    /// Get current available branches (convenience)
    pub fn available_branches(&self) -> &[WorktreeInfo] {
        self.current_repo()
            .map(|r| r.available_branches.as_slice())
            .unwrap_or(&[])
    }

    /// Get current sessions (convenience)
    pub fn sessions(&self) -> &[SessionInfo] {
        self.current_repo()
            .map(|r| r.sessions.as_slice())
            .unwrap_or(&[])
    }

    /// Get current branch_idx (convenience)
    pub fn branch_idx(&self) -> usize {
        self.current_repo().map(|r| r.branch_idx).unwrap_or(0)
    }

    /// Get current worktree (convenience)
    pub fn current_worktree(&self) -> Option<&WorktreeInfo> {
        self.current_repo().and_then(|r| r.current_worktree())
    }

    /// Get current session (convenience)
    pub fn current_session(&self) -> Option<&SessionInfo> {
        self.current_repo().and_then(|r| r.current_session())
    }

    /// Get current line comments (convenience)
    pub fn line_comments(&self) -> &[LineCommentInfo] {
        self.current_repo()
            .map(|r| r.line_comments.as_slice())
            .unwrap_or(&[])
    }

    /// Get current git state (convenience)
    pub fn git(&self) -> Option<&super::state::GitState> {
        self.current_repo().map(|r| &r.git)
    }

    /// Get current git state (mutable, convenience)
    pub fn git_mut(&mut self) -> Option<&mut super::state::GitState> {
        self.current_repo_mut().map(|r| &mut r.git)
    }

    /// Get current diff state (convenience)
    pub fn diff(&self) -> Option<&super::state::DiffState> {
        self.current_repo().map(|r| &r.diff)
    }

    /// Get current diff state (mutable, convenience)
    pub fn diff_mut(&mut self) -> Option<&mut super::state::DiffState> {
        self.current_repo_mut().map(|r| &mut r.diff)
    }

    /// Get add_worktree_idx (convenience)
    pub fn add_worktree_idx(&self) -> usize {
        self.current_repo().map(|r| r.add_worktree_idx).unwrap_or(0)
    }

    // ============ Repo State Mutation Helpers ============

    /// Set branch_idx in current repo
    pub fn set_branch_idx(&mut self, idx: usize) {
        if let Some(repo) = self.current_repo_mut() {
            repo.branch_idx = idx;
        }
    }

    /// Set session_idx in current repo
    pub fn set_session_idx(&mut self, idx: usize) {
        if let Some(repo) = self.current_repo_mut() {
            repo.session_idx = idx;
        }
    }

    /// Set add_worktree_idx in current repo
    pub fn set_add_worktree_idx(&mut self, idx: usize) {
        if let Some(repo) = self.current_repo_mut() {
            repo.add_worktree_idx = idx;
        }
    }

    /// Subscribe to daemon events
    async fn subscribe_events(&mut self) {
        debug!("Subscribing to daemon events");
        match self.client.subscribe_events(None).await {
            Ok(mut stream) => {
                debug!("Event subscription successful");
                let (tx, rx) = mpsc::channel::<DaemonEvent>(64);
                self.event_rx = Some(rx);

                // Spawn task to receive events and forward to channel
                tokio::spawn(async move {
                    while let Some(Ok(event)) = stream.next().await {
                        if tx.send(event).await.is_err() {
                            // Receiver dropped, exit
                            break;
                        }
                    }
                    debug!("Event stream ended");
                });
            }
            Err(e) => {
                // Non-fatal: fall back to polling
                warn!("Failed to subscribe to events: {}", e);
            }
        }
    }

    /// Refresh all data (repos, branches, sessions)
    pub async fn refresh_all(&mut self) -> Result<()> {
        self.error_message = None;

        // Load repos from daemon
        let repos = self.client.list_repos().await?;

        // Update repo_order
        self.repo_order = repos.iter().map(|r| r.id.clone()).collect();

        // Add new repos, keep existing state for known repos
        for repo_info in repos {
            self.repo_states
                .entry(repo_info.id.clone())
                .and_modify(|r| r.info = repo_info.clone())
                .or_insert_with(|| RepoState::new(repo_info));
        }

        // Remove deleted repos
        self.repo_states
            .retain(|id, _| self.repo_order.contains(id));

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Set current repo if not set or if current is no longer valid
        if self.current_repo_id.is_none()
            || !self
                .repo_order
                .contains(self.current_repo_id.as_ref().unwrap_or(&String::new()))
        {
            self.current_repo_id = self.repo_order.first().cloned();
        }

        // Load branches for current repo
        self.refresh_branches().await?;

        Ok(())
    }

    /// Refresh worktrees for current repo
    pub async fn refresh_branches(&mut self) -> Result<()> {
        // Get repo_id first to avoid borrow issues
        let repo_id = match self.current_repo_id.clone() {
            Some(id) => id,
            None => return Ok(()),
        };

        // Fetch worktrees from daemon
        let all_branches = self.client.list_worktrees(&repo_id).await?;

        // Update the repo state
        if let Some(repo) = self.repo_states.get_mut(&repo_id) {
            // Split into worktrees (has path) and available branches (no path)
            repo.worktrees = all_branches
                .iter()
                .filter(|b| !b.path.is_empty())
                .cloned()
                .collect();
            repo.available_branches = all_branches
                .into_iter()
                .filter(|b| b.path.is_empty())
                .collect();

            // Clamp indices to valid ranges
            repo.clamp_indices();

            // Filter expanded worktrees to only include valid indices
            repo.expanded_worktrees
                .retain(|&idx| idx < repo.worktrees.len());

            // Load sessions for expanded worktrees
            let expanded_to_load: Vec<usize> = repo.expanded_worktrees.iter().cloned().collect();
            for wt_idx in expanded_to_load {
                let _ = self.load_worktree_sessions(wt_idx).await;
            }
        }

        // Update sidebar total items count
        self.update_sidebar_total_items();

        // Sidebar cursor is now stored in repo state directly
        // No need to sync since rendering uses repo.sidebar_cursor

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Load sessions for current branch
        self.refresh_sessions().await?;

        // Load git status for current worktree
        let _ = self.load_git_status().await;

        Ok(())
    }

    /// Refresh sessions for current branch
    pub async fn refresh_sessions(&mut self) -> Result<()> {
        // Get repo_id and branch info first to avoid borrow issues
        let (repo_id, branch_name) = {
            if let Some(repo) = self.current_repo() {
                if let Some(wt) = repo.current_worktree() {
                    (repo.info.id.clone(), wt.branch.clone())
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        };

        // Fetch sessions from daemon
        let sessions = self
            .client
            .list_sessions(Some(&repo_id), Some(&branch_name))
            .await?;

        // Update repo state
        if let Some(repo) = self.repo_states.get_mut(&repo_id) {
            repo.sessions = sessions;
            // Clamp session index
            if !repo.sessions.is_empty() && repo.session_idx >= repo.sessions.len() {
                repo.session_idx = repo.sessions.len() - 1;
            }
        }

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Update active session for preview
        self.update_active_session().await;

        Ok(())
    }

    /// Update active session based on current selection
    async fn update_active_session(&mut self) {
        let new_session_id = self.current_session().map(|s| s.id.clone());

        // If session changed, disconnect old stream and connect new one
        if self.terminal.active_session_id != new_session_id {
            self.disconnect_stream();

            // Save current parser to map if there's an active session
            if let Some(old_id) = &self.terminal.active_session_id {
                self.terminal
                    .session_parsers
                    .insert(old_id.clone(), self.terminal.parser.clone());
            }

            // Get or create parser for new session
            if let Some(new_id) = &new_session_id {
                self.terminal.parser = self
                    .terminal
                    .session_parsers
                    .entry(new_id.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                    .clone();
            } else {
                // No session selected, use a fresh parser
                self.terminal.parser = Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)));
            }

            self.terminal.scroll_offset = 0;
            self.terminal.active_session_id = new_session_id;

            // Auto-connect for preview if there's a session
            if self.terminal.active_session_id.is_some() {
                let _ = self.connect_stream().await;
            }
        }
    }

    // ========== Tree view helpers ==========

    /// Update total_items count in sidebar based on current worktrees and expanded state
    /// Note: This is now a no-op since total_items is calculated on-demand
    fn update_sidebar_total_items(&mut self) {
        // Total items are now calculated on-demand via repo.calculate_sidebar_total()
    }

    /// Get the current sidebar item at cursor position
    pub fn current_sidebar_item(&self) -> SidebarItem {
        let Some(repo) = self.current_repo() else {
            return SidebarItem::None;
        };

        let cursor = repo.sidebar_cursor;
        let mut pos = 0;
        for (wt_idx, _wt) in repo.worktrees.iter().enumerate() {
            if pos == cursor {
                return SidebarItem::Worktree(wt_idx);
            }
            pos += 1;
            if repo.expanded_worktrees.contains(&wt_idx) {
                if let Some(sessions) = repo.sessions_by_worktree.get(&wt_idx) {
                    for (s_idx, _session) in sessions.iter().enumerate() {
                        if pos == cursor {
                            return SidebarItem::Session(wt_idx, s_idx);
                        }
                        pos += 1;
                    }
                }
            }
        }
        SidebarItem::None
    }

    /// Toggle expansion of current worktree
    pub fn toggle_sidebar_expand(&mut self) -> Option<AsyncAction> {
        let item = self.current_sidebar_item();
        if let SidebarItem::Worktree(wt_idx) = item {
            if let Some(repo) = self.current_repo_mut() {
                if repo.expanded_worktrees.contains(&wt_idx) {
                    repo.expanded_worktrees.remove(&wt_idx);
                } else {
                    repo.expanded_worktrees.insert(wt_idx);
                    // Load sessions for this worktree if not loaded
                    if !repo.sessions_by_worktree.contains_key(&wt_idx) {
                        self.update_sidebar_total_items();
                        return Some(AsyncAction::LoadWorktreeSessions { wt_idx });
                    }
                }
            }
            self.update_sidebar_total_items();
            self.dirty.sidebar = true;
        }
        None
    }

    /// Move cursor up in sidebar tree view
    pub fn sidebar_move_up(&mut self) -> Option<AsyncAction> {
        let moved = self
            .current_repo_mut()
            .map(|r| r.move_up())
            .unwrap_or(false);
        if moved {
            self.dirty.sidebar = true;
            if self.update_selection_from_sidebar() {
                return Some(AsyncAction::LoadGitStatus);
            }
        }
        None
    }

    /// Move cursor down in sidebar tree view
    pub fn sidebar_move_down(&mut self) -> Option<AsyncAction> {
        let moved = self
            .current_repo_mut()
            .map(|r| r.move_down())
            .unwrap_or(false);
        if moved {
            self.dirty.sidebar = true;
            if self.update_selection_from_sidebar() {
                return Some(AsyncAction::LoadGitStatus);
            }
        }
        None
    }

    /// Update branch_idx and session_idx based on sidebar cursor
    /// Returns true if the worktree changed (needs git status refresh)
    fn update_selection_from_sidebar(&mut self) -> bool {
        let old_branch_idx = self.branch_idx();

        match self.current_sidebar_item() {
            SidebarItem::Worktree(wt_idx) => {
                self.set_branch_idx(wt_idx);
                self.set_session_idx(0);
                // Don't clear active session when navigating to worktree
                // Keep showing the current terminal content
            }
            SidebarItem::Session(wt_idx, s_idx) => {
                self.set_branch_idx(wt_idx);
                self.set_session_idx(s_idx);
                // Get session id from the correct source (RepoState, not SidebarState)
                let session_id = self
                    .current_repo()
                    .and_then(|repo| repo.sessions_by_worktree.get(&wt_idx))
                    .and_then(|sessions| sessions.get(s_idx))
                    .map(|s| s.id.clone());

                if let Some(new_id) = session_id {
                    if self.terminal.active_session_id.as_ref() != Some(&new_id) {
                        self.disconnect_stream();

                        // Save current parser if there was an active session
                        if let Some(old_id) = &self.terminal.active_session_id {
                            self.terminal
                                .session_parsers
                                .insert(old_id.clone(), self.terminal.parser.clone());
                        }

                        // Get or create parser for new session
                        self.terminal.parser = self
                            .terminal
                            .session_parsers
                            .entry(new_id.clone())
                            .or_insert_with(|| {
                                Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)))
                            })
                            .clone();

                        self.terminal.active_session_id = Some(new_id);
                        self.terminal.scroll_offset = 0;
                    }
                }
            }
            SidebarItem::None => {}
        }

        // Return true if worktree changed
        self.branch_idx() != old_branch_idx
    }

    // ========== Sync versions for responsive input handling ==========

    /// Move selection up (sync version - returns async action if needed)
    pub fn select_prev_sync(&mut self) -> Option<AsyncAction> {
        match self.focus {
            Focus::Sidebar => {
                return self.sidebar_move_up();
            }
            Focus::GitStatus => {
                self.git_status_move_up();
            }
            Focus::Terminal => {}
            Focus::DiffFiles => {
                self.diff_move_up();
            }
        }
        None
    }

    /// Move selection down (sync version - returns async action if needed)
    pub fn select_next_sync(&mut self) -> Option<AsyncAction> {
        match self.focus {
            Focus::Sidebar => {
                return self.sidebar_move_down();
            }
            Focus::GitStatus => {
                self.git_status_move_down();
            }
            Focus::Terminal => {}
            Focus::DiffFiles => {
                self.diff_move_down();
            }
        }
        None
    }

    /// Switch to repo by index (sync version)
    /// Saves current repo's view state and restores the target repo's state
    pub fn switch_repo_sync(&mut self, idx: usize) -> Option<AsyncAction> {
        // Get new repo ID from repo_order
        let new_id = self.repo_order.get(idx).cloned();

        // Check if we're actually switching to a different repo
        if new_id.is_some() && new_id != self.current_repo_id {
            // Switch to new repo - state is already preserved in repo_states!
            // Sidebar cursor is stored per-repo, no need to sync
            self.current_repo_id = new_id;

            // Update sidebar total items
            self.update_sidebar_total_items();
            self.dirty.sidebar = true;

            // Refresh branches to ensure data is fresh
            return Some(AsyncAction::RefreshBranches);
        }
        None
    }

    /// Enter terminal Insert mode (from Sessions)
    pub async fn enter_terminal(&mut self) -> Result<()> {
        if self.terminal.active_session_id.is_some() {
            self.focus = Focus::Terminal;
            self.terminal.mode = TerminalMode::Insert;
            self.scroll_to_bottom();

            // Ensure stream is connected
            if self.terminal_stream.is_none() {
                self.connect_stream().await?;
            }
        }
        Ok(())
    }

    /// Enter Insert mode (from Normal mode)
    pub fn enter_insert_mode(&mut self) {
        self.terminal.mode = TerminalMode::Insert;
        self.scroll_to_bottom();
    }

    /// Exit terminal mode (back to sidebar)
    pub fn exit_terminal(&mut self) {
        if self.terminal.fullscreen {
            self.terminal.fullscreen = false;
        } else {
            self.focus = Focus::Sidebar;
            self.terminal.mode = TerminalMode::Normal;
            self.terminal.is_interactive = false;
        }
    }

    /// Toggle between current session and shell session (Ctrl+`)
    pub async fn switch_to_shell_session(&mut self) -> Result<()> {
        const SHELL_SESSION_NAME: &str = "__shell__";

        // Get current worktree
        let current_worktree = match self.current_worktree() {
            Some(wt) => wt.clone(),
            None => {
                self.status_message = Some("No worktree selected".to_string());
                return Ok(());
            }
        };

        // Find shell session in current worktree
        let shell_session = self
            .sessions()
            .iter()
            .find(|s| {
                s.name == SHELL_SESSION_NAME
                    && s.branch == current_worktree.branch
                    && s.is_shell == Some(true)
            })
            .cloned();

        // Check if currently in shell session
        let currently_in_shell = self
            .terminal
            .active_session_id
            .as_ref()
            .and_then(|id| shell_session.as_ref().map(|s| &s.id == id))
            .unwrap_or(false);

        if currently_in_shell {
            // Currently in shell, toggle back to previous session
            if let Some(previous_session_id) = &self.terminal.session_before_shell {
                // Verify previous session still exists
                let previous_session_exists =
                    self.sessions().iter().any(|s| &s.id == previous_session_id);

                if previous_session_exists {
                    let target_id = previous_session_id.clone();

                    // Disconnect current stream
                    self.disconnect_stream();

                    // Save current parser
                    if let Some(old_id) = &self.terminal.active_session_id {
                        self.terminal
                            .session_parsers
                            .insert(old_id.clone(), self.terminal.parser.clone());
                    }

                    // Restore or create parser for target session
                    self.terminal.parser = self
                        .terminal
                        .session_parsers
                        .entry(target_id.clone())
                        .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                        .clone();

                    self.terminal.scroll_offset = 0;
                    self.terminal.active_session_id = Some(target_id);

                    // Clear session_before_shell (we've returned to it)
                    self.terminal.session_before_shell = None;

                    self.enter_terminal().await?;
                    self.status_message = Some("Switched back from shell".to_string());
                } else {
                    // Previous session no longer exists
                    self.terminal.session_before_shell = None;
                    self.status_message = Some("Previous session no longer exists".to_string());
                }
            } else {
                // Already in shell but no previous session saved
                self.status_message = Some("Already in shell session".to_string());
            }
        } else {
            // Not in shell, toggle to shell

            // Save current session ID (if any)
            if let Some(current_id) = &self.terminal.active_session_id {
                self.terminal.session_before_shell = Some(current_id.clone());
            }

            if let Some(session) = shell_session {
                // Shell session exists, switch to it
                let new_id = session.id.clone();

                // Disconnect current stream
                self.disconnect_stream();

                // Save current parser
                if let Some(old_id) = &self.terminal.active_session_id {
                    self.terminal
                        .session_parsers
                        .insert(old_id.clone(), self.terminal.parser.clone());
                }

                // Get or create parser for shell session
                self.terminal.parser = self
                    .terminal
                    .session_parsers
                    .entry(new_id.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                    .clone();

                self.terminal.scroll_offset = 0;
                self.terminal.active_session_id = Some(new_id);

                self.enter_terminal().await?;
                self.status_message = Some("Switched to shell session".to_string());
            } else {
                // Create new shell session
                let repo = match self.current_repo() {
                    Some(r) => r.info.clone(),
                    None => {
                        self.status_message = Some("No repository selected".to_string());
                        return Ok(());
                    }
                };

                match self
                    .client
                    .create_session(
                        &repo.id,
                        &current_worktree.branch,
                        Some(SHELL_SESSION_NAME),
                        Some(true),
                    )
                    .await
                {
                    Ok(session) => {
                        // Refresh sessions list
                        self.refresh_sessions().await?;
                        self.load_worktree_sessions(self.branch_idx()).await?;

                        let new_id = session.id;

                        // Disconnect current stream
                        self.disconnect_stream();

                        // Save current parser
                        if let Some(old_id) = &self.terminal.active_session_id {
                            self.terminal
                                .session_parsers
                                .insert(old_id.clone(), self.terminal.parser.clone());
                        }

                        // Create parser for new shell session
                        self.terminal.parser =
                            Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)));
                        self.terminal
                            .session_parsers
                            .insert(new_id.clone(), self.terminal.parser.clone());

                        self.terminal.scroll_offset = 0;
                        self.terminal.active_session_id = Some(new_id);

                        self.enter_terminal().await?;
                        self.status_message = Some("Created shell session".to_string());
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Failed to create shell session: {}", e));
                    }
                }
            }
        }

        Ok(())
    }

    /// Toggle fullscreen mode
    pub fn toggle_fullscreen(&mut self) {
        self.terminal.fullscreen = !self.terminal.fullscreen;
    }

    /// Scroll up (older content)
    pub fn scroll_up(&mut self, lines: usize) {
        if let Ok(mut parser) = self.terminal.parser.lock() {
            let current = parser.screen().scrollback();
            let new_offset = current + lines;
            parser.screen_mut().set_scrollback(new_offset);
            self.terminal.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll down (newer content)
    pub fn scroll_down(&mut self, lines: usize) {
        if let Ok(mut parser) = self.terminal.parser.lock() {
            let current = parser.screen().scrollback();
            let new_offset = current.saturating_sub(lines);
            parser.screen_mut().set_scrollback(new_offset);
            self.terminal.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        if let Ok(mut parser) = self.terminal.parser.lock() {
            parser.screen_mut().set_scrollback(usize::MAX);
            self.terminal.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        if let Ok(mut parser) = self.terminal.parser.lock() {
            parser.screen_mut().set_scrollback(0);
            self.terminal.scroll_offset = 0;
        }
    }

    /// Create new session and enter interactive mode
    pub async fn create_new(&mut self) -> Result<()> {
        match self.focus {
            Focus::Sidebar => {
                // In tree view: create session for currently selected worktree
                if let (Some(repo), Some(branch)) = (
                    self.current_repo().map(|r| r.info.clone()),
                    self.current_worktree().cloned(),
                ) {
                    match self
                        .client
                        .create_session(&repo.id, &branch.branch, None, None)
                        .await
                    {
                        Ok(session) => {
                            // Refresh sessions for this worktree
                            let b_idx = self.branch_idx();
                            self.load_worktree_sessions(b_idx).await?;
                            // Expand worktree
                            if let Some(repo) = self.current_repo_mut() {
                                repo.expanded_worktrees.insert(b_idx);
                            }
                            self.update_sidebar_total_items();
                            // Set active session before entering terminal
                            self.terminal.active_session_id = Some(session.id.clone());
                            self.enter_terminal().await?;
                        }
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                        }
                    }
                }
            }
            Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => {}
        }
        Ok(())
    }

    /// Submit the input buffer (for new branch creation)
    pub async fn submit_input(&mut self) -> Result<()> {
        if self.input_mode != InputMode::NewBranch {
            return Ok(());
        }

        let branch_name = self.text_input.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.status_message = None;

        if branch_name.is_empty() {
            self.error_message = Some("Branch name cannot be empty".to_string());
            return Ok(());
        }

        // Create session (will auto-create worktree if needed)
        if let Some(repo) = self.current_repo().map(|r| r.info.clone()) {
            match self
                .client
                .create_session(&repo.id, &branch_name, None, None)
                .await
            {
                Ok(session) => {
                    self.refresh_branches().await?;
                    // Find the branch and session
                    if let Some(b_idx) = self
                        .worktrees()
                        .iter()
                        .position(|b| b.branch == branch_name)
                    {
                        self.set_branch_idx(b_idx);
                        self.refresh_sessions().await?;
                        if let Some(s_idx) = self.sessions().iter().position(|s| s.id == session.id)
                        {
                            self.set_session_idx(s_idx);
                            self.update_active_session().await;
                            // Also load sessions for tree view
                            self.load_worktree_sessions(b_idx).await?;
                            if let Some(repo) = self.current_repo_mut() {
                                repo.expanded_worktrees.insert(b_idx);
                            }
                            self.update_sidebar_total_items();
                            self.focus = Focus::Sidebar;
                            self.enter_terminal().await?;
                        }
                    }
                }
                Err(e) => {
                    self.error_message = Some(e.to_string());
                }
            }
        }
        Ok(())
    }

    /// Save current focus before opening a dialog/popup
    pub fn save_focus(&mut self) {
        self.saved_focus_stack.push(self.focus.clone());
    }

    /// Restore focus after closing a dialog/popup
    /// Returns true if focus was restored, false if stack was empty
    pub fn restore_focus(&mut self) -> bool {
        if let Some(saved) = self.saved_focus_stack.pop() {
            self.focus = saved;
            true
        } else {
            // Stack empty - graceful degradation
            #[cfg(debug_assertions)]
            eprintln!("WARNING: Focus stack empty during restore_focus()");
            false
        }
    }

    /// Cancel input mode and restore focus
    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.status_message = None;

        // Restore focus when canceling
        self.restore_focus();
    }

    /// Start add worktree mode
    pub fn start_add_worktree(&mut self) {
        // Get current selected branch as base (None = use HEAD)
        let base_branch = self.current_worktree().map(|w| w.branch.clone());

        self.input_mode = InputMode::AddWorktree { base_branch };
        self.text_input.clear();
        self.set_add_worktree_idx(0);
    }

    /// Start rename session mode
    pub fn start_rename_session(&mut self) {
        if let Some(session) = self.current_session().cloned() {
            self.save_focus();
            self.input_mode = InputMode::RenameSession {
                session_id: session.id.clone(),
            };
            self.text_input.set_content(session.name.clone());
        }
    }

    /// Submit rename session
    pub async fn submit_rename_session(&mut self) -> Result<()> {
        let session_id = match &self.input_mode {
            InputMode::RenameSession { session_id } => session_id.clone(),
            _ => return Ok(()),
        };

        let new_name = self.text_input.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        if new_name.is_empty() {
            self.error_message = Some("Session name cannot be empty".to_string());
            return Ok(());
        }

        match self.client.rename_session(&session_id, &new_name).await {
            Ok(_) => {
                self.status_message = Some(format!("Renamed session to: {}", new_name));
                // Refresh sessions from server to ensure UI is updated
                self.refresh_sessions().await?;
                // Also refresh worktree sessions for tree view
                self.load_worktree_sessions(self.branch_idx()).await?;
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
            }
        }

        Ok(())
    }

    /// Submit add worktree (create worktree for selected or new branch)
    pub async fn submit_add_worktree(&mut self) -> Result<()> {
        // Get base_branch from input mode before clearing
        let base_branch = match &self.input_mode {
            InputMode::AddWorktree { base_branch } => base_branch.clone(),
            _ => None,
        };

        // Determine branch name: typed input or selected from list
        // Only use base_branch when creating a NEW branch (typing in input)
        let (branch_name, use_base) = if !self.text_input.is_empty() {
            // Creating new branch - use base_branch
            (self.text_input.trim().to_string(), true)
        } else if let Some(branch) = self.available_branches().get(self.add_worktree_idx()) {
            // Selecting existing branch - no need for base
            (branch.branch.clone(), false)
        } else {
            self.cancel_input();
            return Ok(());
        };

        let repo_id = match self.current_repo() {
            Some(repo) => repo.info.id.clone(),
            None => {
                self.cancel_input();
                return Ok(());
            }
        };

        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        // Create worktree (pass base_branch only when creating new branch)
        let base = if use_base {
            base_branch.as_deref()
        } else {
            None
        };
        match self
            .client
            .create_worktree(&repo_id, &branch_name, base)
            .await
        {
            Ok(_) => {
                self.status_message = Some(format!("Created worktree for: {}", branch_name));
                self.refresh_branches().await?;
                // Select the new worktree
                if let Some(idx) = self
                    .worktrees()
                    .iter()
                    .position(|w| w.branch == branch_name)
                {
                    self.set_branch_idx(idx);
                    if let Some(repo) = self.current_repo_mut() {
                        repo.sidebar_cursor = idx;
                    }
                    self.refresh_sessions().await?;
                }
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
            }
        }

        Ok(())
    }

    /// Confirm deletion of sessions and worktree
    pub async fn confirm_delete_worktree_sessions(&mut self) -> Result<()> {
        let (repo_id, branch) = match &self.input_mode {
            InputMode::ConfirmDeleteWorktreeSessions {
                repo_id, branch, ..
            } => (repo_id.clone(), branch.clone()),
            _ => return Ok(()),
        };

        // Get sessions for this worktree
        let sessions = self
            .client
            .list_sessions(Some(&repo_id), Some(&branch))
            .await?;

        // Delete all sessions
        for session in sessions {
            // Disconnect if this is the active session
            if self.terminal.active_session_id.as_ref() == Some(&session.id) {
                self.disconnect_stream();
            }
            self.client.destroy_session(&session.id).await?;
        }

        // Now proceed to delete worktree (show confirmation for worktree deletion)
        self.input_mode = InputMode::ConfirmDelete(DeleteTarget::Worktree { repo_id, branch });

        // Refresh sessions to update the UI
        self.refresh_sessions().await?;
        // Also refresh worktree sessions for tree view
        self.load_worktree_sessions(self.branch_idx()).await?;

        Ok(())
    }

    /// Confirm and delete branch (called after worktree deletion)
    pub async fn confirm_delete_branch(&mut self) -> Result<()> {
        let branch_name = match &self.input_mode {
            InputMode::ConfirmDeleteBranch(b) => b.clone(),
            _ => return Ok(()),
        };

        self.input_mode = InputMode::Normal;
        self.restore_focus();

        // Get repo_id
        let repo_id = match self.current_repo() {
            Some(repo) => repo.info.id.clone(),
            None => return Ok(()),
        };

        // Delete branch via daemon
        match self.client.delete_branch(&repo_id, &branch_name).await {
            Ok(_) => {
                self.status_message = Some(format!("Deleted branch: {}", branch_name));
                self.refresh_branches().await?;
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to delete branch: {}", e));
            }
        }

        Ok(())
    }

    /// Request deletion (enters confirm mode)
    pub fn request_delete(&mut self) {
        self.save_focus();
        match self.focus {
            Focus::Sidebar => {
                // In tree view: delete based on current selection
                match self.current_sidebar_item() {
                    SidebarItem::Worktree(wt_idx) => {
                        if let (Some(repo), Some(wt)) = (
                            self.current_repo().map(|r| r.info.clone()),
                            self.worktrees().get(wt_idx).cloned(),
                        ) {
                            if wt.is_main {
                                self.error_message =
                                    Some("Cannot remove main worktree".to_string());
                            } else if wt.path.is_empty() {
                                self.error_message = Some("No worktree to remove".to_string());
                            } else if wt.session_count > 0 {
                                self.input_mode = InputMode::ConfirmDeleteWorktreeSessions {
                                    repo_id: repo.id,
                                    branch: wt.branch,
                                    session_count: wt.session_count,
                                };
                            } else {
                                self.input_mode =
                                    InputMode::ConfirmDelete(DeleteTarget::Worktree {
                                        repo_id: repo.id,
                                        branch: wt.branch,
                                    });
                            }
                        }
                    }
                    SidebarItem::Session(wt_idx, s_idx) => {
                        if let Some(repo) = self.current_repo() {
                            if let Some(sessions) = repo.sessions_by_worktree.get(&wt_idx) {
                                if let Some(session) = sessions.get(s_idx) {
                                    self.input_mode =
                                        InputMode::ConfirmDelete(DeleteTarget::Session {
                                            session_id: session.id.clone(),
                                            name: session.name.clone(),
                                        });
                                }
                            }
                        }
                    }
                    SidebarItem::None => {}
                }
            }
            Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => {}
        }
    }

    /// Confirm and execute deletion
    pub async fn confirm_delete(
        &mut self,
        target: DeleteTarget,
        action: ExitCleanupAction,
    ) -> Result<()> {
        self.input_mode = InputMode::Normal;

        match target {
            DeleteTarget::Worktree { repo_id, branch } => {
                match self.client.remove_worktree(&repo_id, &branch).await {
                    Ok(_) => {
                        // After removing worktree, ask if user wants to delete branch too
                        // Don't restore focus yet - we're chaining to another dialog
                        self.input_mode = InputMode::ConfirmDeleteBranch(branch);
                        self.refresh_branches().await?;
                    }
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                        self.restore_focus();
                    }
                }
            }
            DeleteTarget::Session { session_id, name } => {
                // Disconnect if this is the active session
                if self.terminal.active_session_id.as_ref() == Some(&session_id) {
                    self.disconnect_stream();
                }

                // Execute action based on user selection
                let result = match action {
                    ExitCleanupAction::Destroy => self
                        .client
                        .destroy_session(&session_id)
                        .await
                        .map(|_| format!("Destroyed session: {}", name)),
                    ExitCleanupAction::Stop => self
                        .client
                        .stop_session(&session_id)
                        .await
                        .map(|_| format!("Stopped session: {}", name)),
                };

                match result {
                    Ok(msg) => {
                        self.status_message = Some(msg);
                        self.refresh_sessions().await?;
                        // Also refresh worktree sessions for tree view
                        self.load_worktree_sessions(self.branch_idx()).await?;
                        self.restore_focus();
                    }
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                        self.restore_focus();
                    }
                }
            }
        }

        Ok(())
    }

    /// Connect to session stream for preview/interaction
    pub async fn connect_stream(&mut self) -> Result<()> {
        let session_id = match &self.terminal.active_session_id {
            Some(id) => id.clone(),
            None => return Ok(()),
        };

        // Get terminal size and calculate inner area
        // Layout: Tab bar (3) + Main content + Status bar (3)
        // Main content: Sidebar (25%) + Terminal (75%)
        // Terminal has borders (2 lines, 2 cols)
        let (full_cols, full_rows) = size().map_err(TuiError::TerminalInit)?;
        let main_height = full_rows.saturating_sub(6); // tab + status bars
        let terminal_width = (full_cols as f32 * 0.75) as u16;
        let inner_rows = main_height.saturating_sub(2); // borders
        let inner_cols = terminal_width.saturating_sub(2); // borders

        // Resize vt100 parser to match
        if let Ok(mut parser) = self.terminal.parser.lock() {
            parser.screen_mut().set_size(inner_rows, inner_cols);
        }

        // Create input channel
        let (input_tx, input_rx) = mpsc::channel::<AttachInput>(32);

        // Send initial message with session ID and size
        input_tx
            .send(AttachInput {
                session_id: session_id.clone(),
                data: vec![],
                rows: Some(inner_rows as u32),
                cols: Some(inner_cols as u32),
            })
            .await
            .map_err(|_| TuiError::ChannelSend)?;

        // Start attach stream
        let response = self
            .client
            .inner_mut()
            .attach_session(ReceiverStream::new(input_rx))
            .await?;

        let mut output_stream = response.into_inner();

        // Create output channel
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(64);

        // Spawn task to read from output stream
        tokio::spawn(async move {
            while let Ok(Some(msg)) = output_stream.message().await {
                if output_tx.send(msg.data).await.is_err() {
                    break;
                }
            }
        });

        self.terminal_stream = Some(TerminalStream {
            session_id,
            input_tx,
            output_rx,
        });

        Ok(())
    }

    /// Disconnect from session stream
    pub fn disconnect_stream(&mut self) {
        self.terminal_stream = None;
    }

    /// Check if event subscription needs to be restored
    pub fn needs_resubscribe(&self) -> bool {
        self.event_rx.is_none()
    }

    /// Try to resubscribe to events
    pub async fn try_resubscribe(&mut self) {
        self.subscribe_events().await;
    }

    /// Handle daemon event and return true if UI needs redraw
    fn handle_daemon_event(&mut self, event: DaemonEvent) -> Option<AsyncAction> {
        match event.event {
            Some(daemon_event::Event::SessionCreated(e)) => {
                debug!(
                    "Event: SessionCreated {:?}",
                    e.session.as_ref().map(|s| &s.id)
                );
                if let Some(session) = e.session {
                    // Only add if it matches current repo/branch filter
                    if let (Some(repo), Some(branch)) =
                        (self.current_repo(), self.current_worktree())
                    {
                        if session.repo_id == repo.info.id && session.branch == branch.branch {
                            if let Some(repo) = self.current_repo_mut() {
                                repo.sessions.push(session);
                            }
                            self.dirty.sidebar = true;
                            return None; // Session list changed, will redraw
                        }
                    }
                }
                None
            }
            Some(daemon_event::Event::SessionDestroyed(e)) => {
                debug!("Event: SessionDestroyed {}", e.session_id);
                if let Some(repo) = self.current_repo_mut() {
                    let old_len = repo.sessions.len();
                    // Remove session from list
                    repo.sessions.retain(|s| s.id != e.session_id);
                    // Clamp session index
                    if !repo.sessions.is_empty() && repo.session_idx >= repo.sessions.len() {
                        repo.session_idx = repo.sessions.len() - 1;
                    }
                    // Only redraw if session was actually removed
                    if repo.sessions.len() != old_len {
                        self.dirty.sidebar = true;
                    }
                }
                None
            }
            Some(daemon_event::Event::SessionNameUpdated(e)) => {
                debug!(
                    "Event: SessionNameUpdated {} -> {}",
                    e.session_id, e.new_name
                );
                // Update session name in list
                if let Some(repo) = self.current_repo_mut() {
                    if let Some(session) = repo.sessions.iter_mut().find(|s| s.id == e.session_id) {
                        if session.name != e.new_name {
                            session.name = e.new_name;
                            self.dirty.sidebar = true;
                            return None; // Name changed
                        }
                    }
                }
                None
            }
            Some(daemon_event::Event::SessionStatusChanged(e)) => {
                debug!(
                    "Event: SessionStatusChanged {} {} -> {}",
                    e.session_id, e.old_status, e.new_status
                );
                let mut changed = false;

                // Update session status in main sessions list
                if let Some(repo) = self.current_repo_mut() {
                    if let Some(session) = repo.sessions.iter_mut().find(|s| s.id == e.session_id) {
                        debug!(
                            "Found session in sessions list, current status: {}, new status: {}",
                            session.status, e.new_status
                        );
                        if session.status != e.new_status {
                            debug!(
                                "Status changed! Updating from {} to {}",
                                session.status, e.new_status
                            );
                            session.status = e.new_status;
                            changed = true;
                        }
                    } else {
                        debug!("Session {} not found in sessions list", e.session_id);
                    }
                }

                // Also update in sidebar sessions_by_worktree (for tree view)
                if let Some(repo) = self.current_repo_mut() {
                    for sessions in repo.sessions_by_worktree.values_mut() {
                        if let Some(session) = sessions.iter_mut().find(|s| s.id == e.session_id) {
                            debug!(
                                "Found session in sidebar, updating status from {} to {}",
                                session.status, e.new_status
                            );
                            if session.status != e.new_status {
                                session.status = e.new_status;
                                changed = true;
                            }
                        }
                    }
                }

                if changed {
                    self.dirty.sidebar = true;
                }
                None
            }
            Some(daemon_event::Event::WorktreeAdded(e)) => {
                debug!(
                    "Event: WorktreeAdded {:?}",
                    e.worktree.as_ref().map(|w| &w.branch)
                );
                if let Some(worktree) = e.worktree {
                    // Only add if it matches current repo
                    if let Some(repo) = self.current_repo() {
                        if worktree.repo_id == repo.info.id {
                            // Check if worktree already exists to avoid duplicates
                            if let Some(repo) = self.current_repo_mut() {
                                if !repo.worktrees.iter().any(|w| w.branch == worktree.branch) {
                                    repo.worktrees.push(worktree);
                                    self.dirty.sidebar = true;
                                    return None;
                                }
                            }
                        }
                    }
                }
                None
            }
            Some(daemon_event::Event::WorktreeRemoved(e)) => {
                debug!("Event: WorktreeRemoved {} {}", e.repo_id, e.branch);
                // Remove worktree from list if it matches current repo
                let repo_id_matches = self
                    .current_repo()
                    .map(|r| r.info.id == e.repo_id)
                    .unwrap_or(false);
                if repo_id_matches {
                    if let Some(repo) = self.current_repo_mut() {
                        let old_len = repo.worktrees.len();
                        repo.worktrees.retain(|w| w.branch != e.branch);

                        if repo.worktrees.len() != old_len {
                            // Worktree was removed, update state
                            if repo.worktrees.is_empty() {
                                // All worktrees removed
                                repo.branch_idx = 0;
                                repo.sidebar_cursor = 0;
                                repo.expanded_worktrees.clear();
                                repo.sessions_by_worktree.clear();
                            } else {
                                // Clamp branch index
                                if repo.branch_idx >= repo.worktrees.len() {
                                    repo.branch_idx = repo.worktrees.len() - 1;
                                }
                                // Clear session caches (indices may have shifted)
                                repo.sessions_by_worktree.clear();
                                repo.expanded_worktrees.clear();
                            }

                            // Recalculate sidebar items and clamp cursor
                            let max_cursor = repo.calculate_sidebar_total().saturating_sub(1);
                            if repo.sidebar_cursor > max_cursor {
                                repo.sidebar_cursor = max_cursor;
                            }

                            self.dirty.sidebar = true;
                        }
                    }
                }
                self.update_sidebar_total_items();
                None
            }
            Some(daemon_event::Event::GitStatusChanged(e)) => {
                debug!("Event: GitStatusChanged {}/{}", e.repo_id, e.branch);

                // Only refresh if event is for current worktree
                if let (Some(repo), Some(worktree)) = (self.current_repo(), self.current_worktree())
                {
                    if e.repo_id == repo.info.id && e.branch == worktree.branch {
                        debug!("Auto-refreshing git status for {}/{}", e.repo_id, e.branch);

                        // Client-side debounce: avoid refreshing too frequently
                        if let Some(last) = self.last_git_refresh {
                            if last.elapsed() < std::time::Duration::from_millis(500) {
                                debug!(
                                    "Skipping refresh: debounced (last refresh was {}ms ago)",
                                    last.elapsed().as_millis()
                                );
                                return None; // Skip if refreshed <500ms ago
                            }
                        }

                        self.last_git_refresh = Some(std::time::Instant::now());
                        return Some(AsyncAction::LoadGitStatus);
                    }
                }
                None
            }
            None => None,
        }
    }

    /// Execute a queued async action
    pub async fn execute_async_action(&mut self, action: AsyncAction) -> Result<()> {
        match action {
            AsyncAction::RefreshAll => {
                self.refresh_all().await?;
            }
            AsyncAction::RefreshSessions => {
                let _ = self.refresh_sessions().await;
            }
            AsyncAction::RefreshBranches => {
                let _ = self.refresh_branches().await;
            }
            AsyncAction::CreateSession => {
                self.create_new().await?;
            }
            AsyncAction::SubmitInput => {
                self.submit_input().await?;
            }
            AsyncAction::SubmitRenameSession => {
                self.submit_rename_session().await?;
            }
            AsyncAction::SubmitAddWorktree => {
                self.submit_add_worktree().await?;
            }
            AsyncAction::ConfirmDelete { target, action } => {
                self.confirm_delete(target, action).await?;
            }
            AsyncAction::ConfirmDeleteBranch => {
                self.confirm_delete_branch().await?;
            }
            AsyncAction::ConfirmDeleteWorktreeSessions => {
                self.confirm_delete_worktree_sessions().await?;
            }
            AsyncAction::DestroySession { session_id } => {
                self.client.destroy_session(&session_id).await?;
                let _ = self.refresh_sessions().await;
                // Also refresh worktree sessions for tree view
                let _ = self.load_worktree_sessions(self.branch_idx()).await;
            }
            AsyncAction::RenameSession {
                session_id,
                new_name,
            } => {
                self.client.rename_session(&session_id, &new_name).await?;
            }
            AsyncAction::ConnectStream => {
                self.enter_terminal().await?;
            }
            AsyncAction::ResizeTerminal { rows, cols } => {
                self.resize_terminal(rows, cols).await?;
            }
            AsyncAction::SendToTerminal { data } => {
                self.send_to_terminal(data).await?;
            }
            AsyncAction::SwitchToDiffView => {
                self.switch_to_diff_view().await?;
            }
            AsyncAction::LoadDiffFiles => {
                self.load_diff_files().await?;
            }
            AsyncAction::LoadFileDiff => {
                self.load_file_diff().await?;
            }
            AsyncAction::LoadComments => {
                self.load_comments().await?;
            }
            AsyncAction::SubmitLineComment => {
                self.submit_line_comment().await?;
            }
            AsyncAction::UpdateLineComment => {
                self.update_line_comment().await?;
            }
            AsyncAction::DeleteLineComment => {
                self.delete_current_line_comment().await?;
            }
            AsyncAction::SubmitReviewToClaude => {
                self.submit_review_to_claude().await?;
            }
            AsyncAction::LoadWorktreeSessions { wt_idx } => {
                self.load_worktree_sessions(wt_idx).await?;
            }
            AsyncAction::LoadGitStatus => {
                self.load_git_status().await?;
            }
            AsyncAction::StageFile { file_path } => {
                self.stage_file(&file_path).await?;
            }
            AsyncAction::UnstageFile { file_path } => {
                self.unstage_file(&file_path).await?;
            }
            AsyncAction::StageAll => {
                self.stage_all().await?;
            }
            AsyncAction::UnstageAll => {
                self.unstage_all().await?;
            }
            AsyncAction::SwitchToShell => {
                self.switch_to_shell_session().await?;
            }
            AsyncAction::LoadTodos => {
                self.load_todos().await?;
            }
            AsyncAction::CreateTodo {
                title,
                description,
                parent_id,
            } => {
                self.create_todo(title, description, parent_id).await?;
            }
            AsyncAction::ToggleTodo { todo_id } => {
                self.toggle_todo(&todo_id).await?;
            }
            AsyncAction::DeleteTodo { todo_id } => {
                self.delete_todo(&todo_id).await?;
            }
            AsyncAction::UpdateTodo {
                todo_id,
                title,
                description,
            } => {
                self.update_todo(&todo_id, title, description).await?;
            }
            AsyncAction::ReorderTodo {
                todo_id,
                new_order,
                new_parent_id,
            } => {
                self.reorder_todo(&todo_id, new_order, new_parent_id)
                    .await?;
            }
        }
        Ok(())
    }

    /// Load sessions for a specific worktree (for tree view)
    pub async fn load_worktree_sessions(&mut self, wt_idx: usize) -> Result<()> {
        // Get repo_id and branch first to avoid borrow issues
        let (repo_id, branch) = {
            if let Some(repo) = self.current_repo() {
                if let Some(wt) = repo.worktrees.get(wt_idx) {
                    (repo.info.id.clone(), wt.branch.clone())
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        };

        let sessions = self
            .client
            .list_sessions(Some(&repo_id), Some(&branch))
            .await?;

        // Store sessions in repo state
        if let Some(repo) = self.current_repo_mut() {
            repo.sessions_by_worktree.insert(wt_idx, sessions);
        }
        self.update_sidebar_total_items();
        self.dirty.sidebar = true;
        Ok(())
    }

    // ========== Git Status Methods ==========

    /// Load git status for current worktree
    pub async fn load_git_status(&mut self) -> Result<()> {
        // Get repo_id and branch first to avoid borrow issues
        let (repo_id, branch) = {
            if let Some(repo) = self.current_repo() {
                if let Some(wt) = repo.current_worktree() {
                    (repo.info.id.clone(), wt.branch.clone())
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        };

        let response = self.client.get_git_status(&repo_id, &branch).await?;

        // Update git state in repo
        if let Some(repo) = self.current_repo_mut() {
            repo.git.files.clear();

            for f in response.staged {
                repo.git.files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Staged,
                });
            }
            for f in response.unstaged {
                repo.git.files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Unstaged,
                });
            }
            for f in response.untracked {
                repo.git.files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Untracked,
                });
            }

            repo.git.cursor = 0;
        }
        self.dirty.sidebar = true;

        // Also load comments for this branch
        match self
            .client
            .list_line_comments(&repo_id, &branch, None)
            .await
        {
            Ok(comments) => {
                if let Some(repo) = self.current_repo_mut() {
                    repo.line_comments = comments;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load comments: {}", e);
            }
        }
        Ok(())
    }

    /// Stage a single file
    pub async fn stage_file(&mut self, file_path: &str) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            self.client.stage_file(&repo_id, &branch, file_path).await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Unstage a single file
    pub async fn unstage_file(&mut self, file_path: &str) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            self.client
                .unstage_file(&repo_id, &branch, file_path)
                .await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Stage all files
    pub async fn stage_all(&mut self) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            self.client.stage_all(&repo_id, &branch).await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Unstage all files
    pub async fn unstage_all(&mut self) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            self.client.unstage_all(&repo_id, &branch).await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    // ============ TODO Operations ============

    /// Load TODO items for current repository
    pub async fn load_todos(&mut self) -> Result<()> {
        let repo_id = self.current_repo().map(|r| r.info.id.clone());
        if let Some(repo_id) = repo_id {
            let show_completed = self.todo.show_completed;
            self.todo.items = self.client.list_todos(&repo_id, show_completed).await?;
            self.rebuild_todo_display_order();
        }
        Ok(())
    }

    /// Rebuild display order for TODO items (tree structure)
    pub fn rebuild_todo_display_order(&mut self) {
        use std::collections::HashMap;

        // Build parent-to-children mapping
        let mut items_by_parent: HashMap<Option<String>, Vec<usize>> = HashMap::new();
        for (i, item) in self.todo.items.iter().enumerate() {
            items_by_parent
                .entry(item.parent_id.clone())
                .or_default()
                .push(i);
        }

        // Recursively build display order
        fn build_order(
            items: &[TodoItem],
            items_by_parent: &HashMap<Option<String>, Vec<usize>>,
            parent_id: Option<String>,
            order: &mut Vec<usize>,
        ) {
            if let Some(children) = items_by_parent.get(&parent_id) {
                let mut sorted_children = children.clone();
                sorted_children.sort_by_key(|&idx| items[idx].order);

                for &idx in &sorted_children {
                    order.push(idx);
                    let item = &items[idx];
                    build_order(items, items_by_parent, Some(item.id.clone()), order);
                }
            }
        }

        self.todo.display_order.clear();
        build_order(
            &self.todo.items,
            &items_by_parent,
            None,
            &mut self.todo.display_order,
        );
    }

    /// Create a new TODO item
    pub async fn create_todo(
        &mut self,
        title: String,
        description: Option<String>,
        parent_id: Option<String>,
    ) -> Result<()> {
        let repo_id = self.current_repo().map(|r| r.info.id.clone());
        if let Some(repo_id) = repo_id {
            self.client
                .create_todo(&repo_id, title, description, parent_id)
                .await?;
            self.load_todos().await?;
        }
        Ok(())
    }

    /// Toggle TODO completion status
    pub async fn toggle_todo(&mut self, todo_id: &str) -> Result<()> {
        self.client.toggle_todo(todo_id).await?;
        self.load_todos().await?;
        Ok(())
    }

    /// Delete a TODO item
    pub async fn delete_todo(&mut self, todo_id: &str) -> Result<()> {
        self.client.delete_todo(todo_id).await?;
        self.load_todos().await?;
        Ok(())
    }

    /// Update a TODO item
    pub async fn update_todo(
        &mut self,
        todo_id: &str,
        title: Option<String>,
        description: Option<String>,
    ) -> Result<()> {
        self.client
            .update_todo(todo_id, title, description, None, None)
            .await?;
        self.load_todos().await?;
        Ok(())
    }

    /// Reorder a TODO item
    pub async fn reorder_todo(
        &mut self,
        todo_id: &str,
        new_order: i32,
        new_parent_id: Option<String>,
    ) -> Result<()> {
        self.client
            .reorder_todo(todo_id, new_order, new_parent_id)
            .await?;
        self.load_todos().await?;
        Ok(())
    }

    /// Get current git panel item at cursor position
    pub fn current_git_panel_item(&self) -> GitPanelItem {
        let git = match self.git() {
            Some(g) => g,
            None => return GitPanelItem::None,
        };

        let mut pos = 0;
        let sections = [
            GitSection::Staged,
            GitSection::Unstaged,
            GitSection::Untracked,
        ];

        for section in sections {
            let files: Vec<_> = git
                .files
                .iter()
                .enumerate()
                .filter(|(_, f)| f.section == section)
                .collect();

            if files.is_empty() {
                continue;
            }

            // Section header
            if pos == git.cursor {
                return GitPanelItem::Section(section);
            }
            pos += 1;

            // Files in section (if expanded)
            if git.expanded_sections.contains(&section) {
                for (file_idx, _) in files {
                    if pos == git.cursor {
                        return GitPanelItem::File(file_idx);
                    }
                    pos += 1;
                }
            }
        }

        GitPanelItem::None
    }

    /// Toggle git section expansion
    pub fn toggle_git_section_expand(&mut self) {
        if let GitPanelItem::Section(section) = self.current_git_panel_item() {
            if let Some(git) = self.git_mut() {
                if git.expanded_sections.contains(&section) {
                    git.expanded_sections.remove(&section);
                } else {
                    git.expanded_sections.insert(section);
                }
                self.dirty.sidebar = true;
            }
        }
    }

    /// Move cursor up in git status panel
    pub fn git_status_move_up(&mut self) {
        if let Some(git) = self.git_mut() {
            if git.move_up() {
                self.dirty.sidebar = true;
            }
        }
    }

    /// Move cursor down in git status panel
    pub fn git_status_move_down(&mut self) {
        if let Some(git) = self.git_mut() {
            if git.move_down() {
                self.dirty.sidebar = true;
            }
        }
    }

    /// Get file path of currently selected git status file
    pub fn current_git_file_path(&self) -> Option<String> {
        if let GitPanelItem::File(idx) = self.current_git_panel_item() {
            self.git()?.files.get(idx).map(|f| f.path.clone())
        } else {
            None
        }
    }

    /// Check if current git item is staged
    pub fn is_current_git_item_staged(&self) -> bool {
        if let GitPanelItem::File(idx) = self.current_git_panel_item() {
            self.git()
                .and_then(|git| git.files.get(idx))
                .map(|f| f.section == GitSection::Staged)
                .unwrap_or(false)
        } else if let GitPanelItem::Section(section) = self.current_git_panel_item() {
            section == GitSection::Staged
        } else {
            false
        }
    }

    /// Send data to terminal
    pub async fn send_to_terminal(&mut self, data: Vec<u8>) -> Result<()> {
        if let Some(stream) = &self.terminal_stream {
            stream
                .input_tx
                .send(AttachInput {
                    session_id: stream.session_id.clone(),
                    data,
                    rows: None,
                    cols: None,
                })
                .await
                .map_err(|_| TuiError::ChannelSend)?;
        }
        Ok(())
    }

    /// Send resize to terminal
    pub async fn resize_terminal(&mut self, rows: u16, cols: u16) -> Result<()> {
        // Store terminal size for mouse position calculations
        self.terminal.cols = Some(cols);
        self.terminal.rows = Some(rows);

        // Calculate inner area (same as connect_stream)
        let main_height = rows.saturating_sub(6);
        let terminal_width = (cols as f32 * 0.75) as u16;
        let inner_rows = main_height.saturating_sub(2);
        let inner_cols = terminal_width.saturating_sub(2);

        // Resize parser
        if let Ok(mut parser) = self.terminal.parser.lock() {
            parser.screen_mut().set_size(inner_rows, inner_cols);
        }

        if let Some(stream) = &self.terminal_stream {
            stream
                .input_tx
                .send(AttachInput {
                    session_id: stream.session_id.clone(),
                    data: vec![],
                    rows: Some(inner_rows as u32),
                    cols: Some(inner_cols as u32),
                })
                .await
                .map_err(|_| TuiError::ChannelSend)?;
        }
        Ok(())
    }

    // ========== Diff View ==========

    /// Switch to diff view
    pub async fn switch_to_diff_view(&mut self) -> Result<()> {
        self.right_panel_view = RightPanelView::Diff;
        self.focus = Focus::DiffFiles;
        self.load_diff_files().await?;
        self.load_comments().await?;
        Ok(())
    }

    /// Switch back to terminal view
    pub fn switch_to_terminal_view(&mut self) {
        self.right_panel_view = RightPanelView::Terminal;
        self.focus = Focus::Sidebar;
        if let Some(diff) = self.diff_mut() {
            diff.files.clear();
            diff.expanded.clear();
            diff.file_lines.clear();
            diff.cursor = 0;
            diff.scroll_offset = 0;
        }
    }

    /// Load diff files for current worktree
    pub async fn load_diff_files(&mut self) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            match self.client.get_diff_files(&repo_id, &branch).await {
                Ok(files) => {
                    // Get pending file before modifying state
                    let pending_file = self.git_mut().and_then(|g| g.pending_diff_file.take());

                    if let Some(diff) = self.diff_mut() {
                        diff.files = files;
                        diff.expanded.clear();
                        diff.file_lines.clear();
                        diff.cursor = 0;
                        diff.scroll_offset = 0;

                        // If there's a pending file to expand, find and expand it
                        if let Some(pending_file) = pending_file {
                            if let Some(idx) =
                                diff.files.iter().position(|f| f.path == pending_file)
                            {
                                diff.cursor = idx;
                                diff.expanded.insert(idx);
                            }
                        }
                    }

                    // Load the file's diff content if we just expanded one
                    self.load_file_diff().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to load diff: {}", e));
                }
            }

            // Also load comments for this branch
            match self
                .client
                .list_line_comments(&repo_id, &branch, None)
                .await
            {
                Ok(comments) => {
                    if let Some(repo) = self.current_repo_mut() {
                        repo.line_comments = comments;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load comments: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Load diff content for the file that is being expanded
    pub async fn load_file_diff(&mut self) -> Result<()> {
        // Find which file needs loading (the one that's expanded but has no lines)
        let file_info = self.diff().and_then(|diff| {
            diff.expanded
                .iter()
                .find(|&&idx| !diff.file_lines.contains_key(&idx))
                .copied()
                .and_then(|idx| diff.files.get(idx).map(|f| (idx, f.path.clone())))
        });

        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let (Some((file_idx, file_path)), Some((repo_id, branch))) = (file_info, ids) {
            match self
                .client
                .get_file_diff(&repo_id, &branch, &file_path)
                .await
            {
                Ok(response) => {
                    if let Some(diff) = self.diff_mut() {
                        diff.file_lines.insert(file_idx, response.lines);
                    }
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to load file diff: {}", e));
                    if let Some(diff) = self.diff_mut() {
                        diff.expanded.remove(&file_idx);
                    }
                }
            }
        }
        Ok(())
    }

    // ========== Unified diff navigation ==========

    /// Get current item at cursor position
    pub fn current_diff_item(&self) -> DiffItem {
        let Some(diff) = self.diff() else {
            return DiffItem::None;
        };

        if diff.files.is_empty() {
            return DiffItem::None;
        }

        let mut pos = 0;
        for (file_idx, _) in diff.files.iter().enumerate() {
            // Check if cursor is on this file
            if pos == diff.cursor {
                return DiffItem::File(file_idx);
            }
            pos += 1;

            // Check if cursor is on one of this file's lines
            if diff.expanded.contains(&file_idx) {
                if let Some(lines) = diff.file_lines.get(&file_idx) {
                    for line_idx in 0..lines.len() {
                        if pos == diff.cursor {
                            return DiffItem::Line(file_idx, line_idx);
                        }
                        pos += 1;
                    }
                }
            }
        }

        DiffItem::None
    }

    /// Move cursor up in diff view
    pub fn diff_move_up(&mut self) {
        if let Some(diff) = self.diff_mut() {
            if diff.move_up() {
                self.dirty.sidebar = true;
            }
        }
    }

    /// Move cursor down in diff view
    pub fn diff_move_down(&mut self) {
        if let Some(diff) = self.diff_mut() {
            if diff.move_down() {
                self.dirty.sidebar = true;
            }
        }
    }

    /// Jump to previous file
    pub fn diff_prev_file(&mut self) {
        let Some(diff) = self.diff_mut() else { return };

        let mut pos = 0;
        let mut last_file_pos = 0;
        for (file_idx, _) in diff.files.iter().enumerate() {
            if pos >= diff.cursor {
                // Found current or past cursor, go to last file
                break;
            }
            last_file_pos = pos;
            pos += 1;
            if diff.expanded.contains(&file_idx) {
                pos += diff.file_lines.get(&file_idx).map(|l| l.len()).unwrap_or(0);
            }
        }
        if diff.cursor > 0 {
            diff.cursor = last_file_pos;
            self.dirty.sidebar = true;
        }
    }

    /// Jump to next file
    pub fn diff_next_file(&mut self) {
        let Some(diff) = self.diff_mut() else { return };

        let mut pos = 0;
        for (file_idx, _) in diff.files.iter().enumerate() {
            if pos > diff.cursor {
                // Found next file after cursor
                diff.cursor = pos;
                self.dirty.sidebar = true;
                return;
            }
            pos += 1;
            if diff.expanded.contains(&file_idx) {
                pos += diff.file_lines.get(&file_idx).map(|l| l.len()).unwrap_or(0);
            }
        }
    }

    /// Toggle expansion of current file (only works when cursor is on a file)
    pub fn toggle_diff_expand(&mut self) -> Option<AsyncAction> {
        if let DiffItem::File(file_idx) = self.current_diff_item() {
            let diff = self.diff_mut()?;
            if diff.expanded.contains(&file_idx) {
                // Collapse
                diff.expanded.remove(&file_idx);
                diff.file_lines.remove(&file_idx);
                None
            } else {
                // Expand - need to load diff content
                diff.expanded.insert(file_idx);
                Some(AsyncAction::LoadFileDiff)
            }
        } else {
            None
        }
    }

    /// Toggle diff fullscreen mode
    pub fn toggle_diff_fullscreen(&mut self) {
        if let Some(diff) = self.diff_mut() {
            diff.fullscreen = !diff.fullscreen;
        }
    }

    // ========== Comment Operations ==========

    /// Start adding a line comment (only works when cursor is on a diff line)
    pub fn start_add_line_comment(&mut self) {
        if let DiffItem::Line(file_idx, line_idx) = self.current_diff_item() {
            let Some(diff) = self.diff() else { return };
            if let (Some(file), Some(lines)) = (
                diff.files.get(file_idx).cloned(),
                diff.file_lines.get(&file_idx),
            ) {
                if let Some(diff_line) = lines.get(line_idx).cloned() {
                    // Get actual line number from diff info
                    let line_number = diff_line
                        .new_lineno
                        .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));

                    self.save_focus();
                    self.input_mode = InputMode::AddLineComment {
                        file_path: file.path.clone(),
                        line_number,
                        line_type: diff_line.line_type,
                    };
                    self.text_input.clear();
                }
            }
        } else {
            self.status_message = Some("Move cursor to a diff line to add comment".to_string());
        }
    }

    /// Submit the current line comment
    pub async fn submit_line_comment(&mut self) -> Result<()> {
        let (file_path, line_number, line_type) = match &self.input_mode {
            InputMode::AddLineComment {
                file_path,
                line_number,
                line_type,
            } => (file_path.clone(), *line_number, *line_type),
            _ => return Ok(()),
        };

        let comment_text = self.text_input.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        if comment_text.is_empty() {
            self.status_message = Some("Comment cannot be empty".to_string());
            return Ok(());
        }

        // Get current repo and branch
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            match self
                .client
                .create_line_comment(
                    &repo_id,
                    &branch,
                    &file_path,
                    line_number,
                    line_type,
                    &comment_text,
                )
                .await
            {
                Ok(comment) => {
                    if let Some(repo) = self.current_repo_mut() {
                        repo.line_comments.push(comment);
                    }
                    self.status_message = Some("Comment added".to_string());
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to add comment: {}", e));
                }
            }
        }

        Ok(())
    }

    /// Load comments for current branch
    pub async fn load_comments(&mut self) -> Result<()> {
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            match self
                .client
                .list_line_comments(&repo_id, &branch, None)
                .await
            {
                Ok(comments) => {
                    if let Some(repo) = self.current_repo_mut() {
                        repo.line_comments = comments;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load comments: {}", e);
                    if let Some(repo) = self.current_repo_mut() {
                        repo.line_comments.clear();
                    }
                }
            }
        }
        Ok(())
    }

    /// Get comments for a specific file and line
    pub fn get_line_comment(&self, file_path: &str, line_number: i32) -> Option<&LineCommentInfo> {
        self.line_comments()
            .iter()
            .find(|c| c.file_path == file_path && c.line_number == line_number)
    }

    /// Check if a line has a comment
    pub fn has_line_comment(&self, file_path: &str, line_number: i32) -> bool {
        self.get_line_comment(file_path, line_number).is_some()
    }

    /// Count comments for a specific file
    pub fn count_file_comments(&self, file_path: &str) -> usize {
        self.line_comments()
            .iter()
            .filter(|c| c.file_path == file_path)
            .count()
    }

    /// Start editing an existing comment on current line
    pub fn start_edit_line_comment(&mut self) {
        // Extract needed data first to avoid borrow conflicts
        let edit_info: Option<(String, String, i32, String)> = {
            if let DiffItem::Line(file_idx, line_idx) = self.current_diff_item() {
                let diff = self.diff();
                if let (Some(file), Some(lines)) = (
                    diff.and_then(|d| d.files.get(file_idx)),
                    diff.and_then(|d| d.file_lines.get(&file_idx)),
                ) {
                    if let Some(diff_line) = lines.get(line_idx) {
                        let line_number = diff_line
                            .new_lineno
                            .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));

                        // Check if there's a comment on this line
                        self.get_line_comment(&file.path, line_number)
                            .map(|comment| {
                                (
                                    comment.id.clone(),
                                    file.path.clone(),
                                    line_number,
                                    comment.comment.clone(),
                                )
                            })
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Now mutate self
        if let Some((comment_id, file_path, line_number, comment_text)) = edit_info {
            self.save_focus();
            self.input_mode = InputMode::EditLineComment {
                comment_id,
                file_path,
                line_number,
            };
            self.text_input.set_content(comment_text);
        } else {
            self.status_message = Some("No comment on this line to edit".to_string());
        }
    }

    /// Update an existing line comment
    pub async fn update_line_comment(&mut self) -> Result<()> {
        let comment_id = match &self.input_mode {
            InputMode::EditLineComment { comment_id, .. } => comment_id.clone(),
            _ => return Ok(()),
        };

        let comment_text = self.text_input.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        if comment_text.is_empty() {
            self.status_message = Some("Comment cannot be empty".to_string());
            return Ok(());
        }

        match self
            .client
            .update_line_comment(&comment_id, &comment_text)
            .await
        {
            Ok(updated) => {
                // Update in local list
                if let Some(repo) = self.current_repo_mut() {
                    if let Some(comment) =
                        repo.line_comments.iter_mut().find(|c| c.id == comment_id)
                    {
                        comment.comment = updated.comment;
                    }
                }
                self.status_message = Some("Comment updated".to_string());
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to update comment: {}", e));
            }
        }

        Ok(())
    }

    /// Delete comment on current line
    pub async fn delete_current_line_comment(&mut self) -> Result<()> {
        if let DiffItem::Line(file_idx, line_idx) = self.current_diff_item() {
            let Some(diff) = self.diff() else {
                return Ok(());
            };
            if let (Some(file), Some(lines)) =
                (diff.files.get(file_idx), diff.file_lines.get(&file_idx))
            {
                if let Some(diff_line) = lines.get(line_idx) {
                    let line_number = diff_line
                        .new_lineno
                        .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));

                    if let Some(comment) = self
                        .line_comments()
                        .iter()
                        .find(|c| c.file_path == file.path && c.line_number == line_number)
                    {
                        let comment_id = comment.id.clone();

                        match self.client.delete_line_comment(&comment_id).await {
                            Ok(_) => {
                                if let Some(repo) = self.current_repo_mut() {
                                    repo.line_comments.retain(|c| c.id != comment_id);
                                }
                                self.status_message = Some("Comment deleted".to_string());
                            }
                            Err(e) => {
                                self.error_message =
                                    Some(format!("Failed to delete comment: {}", e));
                            }
                        }
                        return Ok(());
                    }
                }
            }
        }
        self.status_message = Some("No comment on this line to delete".to_string());
        Ok(())
    }

    /// Jump to next line with a comment
    pub fn jump_to_next_comment(&mut self) {
        let current = self.current_diff_item();
        let (current_file_idx, current_line_idx) = match current {
            DiffItem::File(f) => (f, 0),
            DiffItem::Line(f, l) => (f, l),
            DiffItem::None => return,
        };

        // Build a flat list of (file_idx, line_idx, line_number, file_path)
        let all_lines: Vec<(usize, usize, i32, String)> = {
            let Some(diff) = self.diff() else { return };
            let mut lines = Vec::new();
            for (file_idx, file) in diff.files.iter().enumerate() {
                if diff.expanded.contains(&file_idx) {
                    if let Some(diff_lines) = diff.file_lines.get(&file_idx) {
                        for (line_idx, diff_line) in diff_lines.iter().enumerate() {
                            let line_number = diff_line
                                .new_lineno
                                .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));
                            lines.push((file_idx, line_idx, line_number, file.path.clone()));
                        }
                    }
                }
            }
            lines
        };

        // Find current position in flat list
        let current_pos = all_lines
            .iter()
            .position(|(f, l, _, _)| *f == current_file_idx && *l >= current_line_idx)
            .unwrap_or(0);

        // Find next comment after current position
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().skip(current_pos + 1) {
            if self.has_line_comment(file_path, *line_number) {
                let cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                if let Some(diff) = self.diff_mut() {
                    diff.cursor = cursor;
                }
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        // Wrap around - search from beginning
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().take(current_pos + 1) {
            if self.has_line_comment(file_path, *line_number) {
                let cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                if let Some(diff) = self.diff_mut() {
                    diff.cursor = cursor;
                }
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        self.status_message = Some("No comments to jump to".to_string());
    }

    /// Jump to previous line with a comment
    pub fn jump_to_prev_comment(&mut self) {
        let current = self.current_diff_item();
        let (current_file_idx, current_line_idx) = match current {
            DiffItem::File(f) => (f, 0),
            DiffItem::Line(f, l) => (f, l),
            DiffItem::None => return,
        };

        // Build a flat list of (file_idx, line_idx, line_number, file_path)
        let all_lines: Vec<(usize, usize, i32, String)> = {
            let Some(diff) = self.diff() else { return };
            let mut lines = Vec::new();
            for (file_idx, file) in diff.files.iter().enumerate() {
                if diff.expanded.contains(&file_idx) {
                    if let Some(diff_lines) = diff.file_lines.get(&file_idx) {
                        for (line_idx, diff_line) in diff_lines.iter().enumerate() {
                            let line_number = diff_line
                                .new_lineno
                                .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));
                            lines.push((file_idx, line_idx, line_number, file.path.clone()));
                        }
                    }
                }
            }
            lines
        };

        // Find current position in flat list
        let current_pos = all_lines
            .iter()
            .position(|(f, l, _, _)| *f == current_file_idx && *l >= current_line_idx)
            .unwrap_or(all_lines.len());

        // Find previous comment before current position
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().take(current_pos).rev()
        {
            if self.has_line_comment(file_path, *line_number) {
                let cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                if let Some(diff) = self.diff_mut() {
                    diff.cursor = cursor;
                }
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        // Wrap around - search from end
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().skip(current_pos).rev()
        {
            if self.has_line_comment(file_path, *line_number) {
                let cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                if let Some(diff) = self.diff_mut() {
                    diff.cursor = cursor;
                }
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        self.status_message = Some("No comments to jump to".to_string());
    }

    /// Calculate cursor position for a specific file and line
    fn calculate_cursor_for_line(&self, target_file_idx: usize, target_line_idx: usize) -> usize {
        let Some(diff) = self.diff() else { return 0 };
        let mut cursor = 0;
        for (file_idx, _) in diff.files.iter().enumerate() {
            if file_idx == target_file_idx {
                // Found the file, add the line offset
                return cursor + 1 + target_line_idx; // +1 for file header
            }
            cursor += 1; // File header
            if diff.expanded.contains(&file_idx) {
                if let Some(lines) = diff.file_lines.get(&file_idx) {
                    cursor += lines.len();
                }
            }
        }
        cursor
    }

    /// Submit all comments as a review to Claude
    pub async fn submit_review_to_claude(&mut self) -> Result<()> {
        if self.line_comments().is_empty() {
            self.status_message = Some("No comments to submit".to_string());
            return Ok(());
        }

        // Build the review prompt
        let mut prompt = String::from("Please help me review the following code changes:\n\n");

        // Group comments by file
        let mut by_file: std::collections::HashMap<String, Vec<&LineCommentInfo>> =
            std::collections::HashMap::new();
        for comment in self.line_comments() {
            by_file
                .entry(comment.file_path.clone())
                .or_default()
                .push(comment);
        }

        for (file_path, comments) in by_file {
            prompt.push_str(&format!("## File: {}\n\n", file_path));

            for comment in comments {
                let line_type_str = match comment.line_type {
                    3 => "+", // Addition
                    4 => "-", // Deletion
                    _ => " ", // Context
                };

                prompt.push_str(&format!(
                    "### Line {} ({})\n",
                    comment.line_number, line_type_str
                ));
                prompt.push_str(&format!("Comment: {}\n\n", comment.comment));
            }
        }

        prompt.push_str("---\nPlease provide your suggestions for the above comments.\n");

        // Switch to terminal view and send to PTY
        self.switch_to_terminal_view();

        // Connect if needed and send
        if self.terminal_stream.is_none() && self.terminal.active_session_id.is_some() {
            self.enter_terminal().await?;
        }

        if self.terminal_stream.is_some() {
            self.send_to_terminal(prompt.into_bytes()).await?;
            self.status_message = Some("Review sent to Claude".to_string());
        } else {
            self.error_message = Some("No active session to send review".to_string());
        }

        Ok(())
    }
}

// Drop trait for automatic cleanup on abnormal exit
impl Drop for App {
    fn drop(&mut self) {
        // Only cleanup on abnormal exit (should_quit = false means unexpected termination)
        if !self.should_quit && !self.sessions().is_empty() {
            let running_ids: Vec<String> = self
                .sessions()
                .iter()
                .filter(|s| s.status == 1) // SESSION_STATUS_RUNNING
                .map(|s| s.id.clone())
                .collect();

            if !running_ids.is_empty() {
                // Drop cannot be async, so create a sync runtime
                if let Ok(runtime) = tokio::runtime::Runtime::new() {
                    for session_id in &running_ids {
                        let _ = runtime.block_on(self.client.stop_session(session_id));
                    }
                }
            }
        }
    }
}

/// Result of TUI run
pub enum RunResult {
    /// User quit (q)
    Quit,
}

/// Spawn a thread to read crossterm events (blocking I/O)
fn spawn_input_reader() -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel(32);

    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            if tx.blocking_send(event).is_err() {
                break; // Receiver dropped
            }
        }
    });

    rx
}

/// Run the TUI application
pub async fn run_with_client(mut app: App) -> Result<RunResult> {
    // Deactivate IME at startup
    deactivate_ime();

    // Setup terminal
    enable_raw_mode().map_err(TuiError::TerminalInit)?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(TuiError::TerminalInit)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(TuiError::TerminalInit)?;

    // Spawn input reader thread (crossterm events are blocking)
    let mut input_rx = spawn_input_reader();

    // Fixed 16ms render interval (~60fps) - always render on every tick (tuitest pattern)
    let mut render_interval = tokio::time::interval(std::time::Duration::from_millis(16));
    render_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Fallback timers for daemon reconnection
    let mut last_refresh = std::time::Instant::now();
    let mut last_resubscribe_attempt = std::time::Instant::now();
    let refresh_interval = std::time::Duration::from_secs(5);
    let resubscribe_interval = std::time::Duration::from_secs(10);

    // Only need pending_action for async operations
    let mut pending_action: Option<AsyncAction> = None;

    // Main loop with tokio::select!
    loop {
        tokio::select! {
            biased; // Check branches in priority order

            // 1. Highest priority: keyboard input
            Some(event) = input_rx.recv() => {
                match event {
                    Event::Key(key) => {
                        // Sync input handling - returns optional async action
                        if let Some(action) = handle_input_sync(&mut app, key) {
                            // If already have a pending action, execute it immediately
                            if let Some(old_action) = pending_action.take() {
                                let _ = app.execute_async_action(old_action).await;
                            }
                            pending_action = Some(action);
                        }
                    }
                    Event::Resize(cols, rows) => {
                        let _ = app.resize_terminal(rows, cols).await;
                    }
                    Event::Mouse(mouse) => {
                        handle_mouse_sync(&mut app, mouse);
                    }
                    _ => {}
                }
            }

            // 2. Terminal PTY output - just process data, no flags needed
            Some(data) = async {
                match app.terminal_stream.as_mut() {
                    Some(stream) => stream.output_rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Ok(mut parser) = app.terminal.parser.lock() {
                    parser.process(&data);
                }
            }

            // 3. Daemon events - update state
            Some(event) = async {
                match app.event_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(action) = app.handle_daemon_event(event) {
                    // If already have a pending action, execute it immediately
                    if let Some(old_action) = pending_action.take() {
                        let _ = app.execute_async_action(old_action).await;
                    }
                    pending_action = Some(action);
                }
            }

            // 4. Render tick - ALWAYS RENDER (tuitest pattern)
            _ = render_interval.tick() => {
                // Execute pending async action
                if let Some(action) = pending_action.take() {
                    if let Err(e) = app.execute_async_action(action).await {
                        app.error_message = Some(format!("{}", e));
                    }
                }

                // Check if we need to resubscribe (event channel disconnected)
                if app.needs_resubscribe() {
                    // Fallback: Periodic session refresh while disconnected
                    if last_refresh.elapsed() >= refresh_interval {
                        let _ = app.refresh_sessions().await;
                        last_refresh = std::time::Instant::now();
                    }

                    // Periodically attempt to resubscribe
                    if last_resubscribe_attempt.elapsed() >= resubscribe_interval {
                        app.try_resubscribe().await;
                        last_resubscribe_attempt = std::time::Instant::now();
                    }
                }

                // Always render - no dirty checks needed (tuitest pattern)
                // Use synchronized update to prevent flicker
                execute!(terminal.backend_mut(), BeginSynchronizedUpdate)
                    .map_err(TuiError::Render)?;
                terminal.draw(|f| draw(f, &app)).map_err(TuiError::Render)?;
                execute!(terminal.backend_mut(), EndSynchronizedUpdate)
                    .map_err(TuiError::Render)?;
            }
        }

        // Check if should quit
        if app.should_quit {
            break;
        }
    }

    // Cleanup
    app.disconnect_stream();

    // Restore terminal
    disable_raw_mode().map_err(TuiError::TerminalRestore)?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .map_err(TuiError::TerminalRestore)?;
    terminal.show_cursor().map_err(TuiError::TerminalRestore)?;

    // Activate IME at exit
    activate_ime();

    Ok(RunResult::Quit)
}
