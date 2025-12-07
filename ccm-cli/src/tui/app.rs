//! TUI application state machine

use crate::client::Client;
use crate::error::TuiError;
use ccm_proto::daemon::{
    event as daemon_event, AttachInput, DiffFileInfo, DiffLine, Event as DaemonEvent,
    LineCommentInfo, RepoInfo, SessionInfo, TodoItem, WorktreeInfo,
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
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{debug, warn};

type Result<T> = std::result::Result<T, TuiError>;

use super::input::{handle_input_sync, handle_mouse_sync};
use super::state::{
    default_expanded_git_sections, AsyncAction, DeleteTarget, DiffItem, DirtyFlags, Focus,
    GitPanelItem, GitSection, GitStatusFile, InputMode, PrefixMode, RightPanelView, SidebarItem,
    TerminalMode,
};
use super::ui::draw;

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

    // Selection indices
    pub repo_idx: usize,
    pub branch_idx: usize,
    pub session_idx: usize,

    // Focus position
    pub focus: Focus,

    // Data
    pub repos: Vec<RepoInfo>,
    pub worktrees: Vec<WorktreeInfo>, // Only branches with worktrees
    pub available_branches: Vec<WorktreeInfo>, // Branches without worktrees (for add worktree)
    pub add_worktree_idx: usize,      // Selection index in add worktree popup
    pub sessions: Vec<SessionInfo>,

    // Terminal state
    pub terminal_parser: Arc<Mutex<vt100::Parser>>,
    pub session_parsers: HashMap<String, Arc<Mutex<vt100::Parser>>>, // Per-session parsers
    pub active_session_id: Option<String>,
    pub is_interactive: bool,
    pub terminal_stream: Option<TerminalStream>,
    pub terminal_mode: TerminalMode,
    pub scroll_offset: usize,
    pub terminal_fullscreen: bool,
    pub last_terminal_hash: u64, // Hash of terminal content for change detection
    pub cached_terminal_lines: Vec<ratatui::text::Line<'static>>, // Cached terminal lines
    pub cached_terminal_size: (u16, u16), // (height, width) for cache invalidation

    // Diff state
    pub right_panel_view: RightPanelView,
    pub diff_files: Vec<DiffFileInfo>,
    pub diff_expanded: std::collections::HashSet<usize>, // Which files are expanded
    pub diff_file_lines: std::collections::HashMap<usize, Vec<DiffLine>>, // Lines per expanded file
    pub diff_cursor: usize,                              // Unified cursor position in virtual list
    pub diff_scroll_offset: usize,                       // Scroll offset for rendering
    pub diff_fullscreen: bool,

    // Comment state
    pub line_comments: Vec<LineCommentInfo>, // All comments for current branch

    // Tree view state (for sidebar)
    pub tree_view_enabled: bool, // Enable tree view mode
    pub expanded_worktrees: std::collections::HashSet<usize>, // Which worktrees are expanded
    pub sidebar_cursor: usize,   // Cursor position in virtual list
    pub sessions_by_worktree: HashMap<usize, Vec<SessionInfo>>, // Sessions grouped by worktree index

    // Git status panel state
    pub git_panel_enabled: bool,
    pub git_status_files: Vec<GitStatusFile>, // All files (staged + unstaged + untracked)
    pub git_status_cursor: usize,             // Cursor in virtual list
    pub expanded_git_sections: std::collections::HashSet<GitSection>, // Expanded sections
    #[allow(dead_code)]
    pub git_panel_scroll_offset: usize, // Scroll offset for rendering
    pub pending_diff_file: Option<String>,    // File to auto-expand in diff view

    // Terminal size tracking (for mouse position calculations)
    pub terminal_cols: Option<u16>,
    pub terminal_rows: Option<u16>,

    // UI state
    pub should_quit: bool,
    pub error_message: Option<String>,
    pub status_message: Option<String>,
    pub input_mode: InputMode,
    pub input_buffer: String,

    // Event subscription
    pub event_rx: Option<mpsc::Receiver<DaemonEvent>>,

    // Prefix key mode
    pub prefix_mode: PrefixMode,

    // Dirty flags for optimized rendering
    pub dirty: DirtyFlags,

    // TODO state
    pub todo_items: Vec<TodoItem>,
    pub todo_cursor: usize,
    pub expanded_todos: std::collections::HashSet<String>,
    pub todo_scroll_offset: usize,
    pub todo_show_completed: bool,
    pub todo_display_order: Vec<usize>, // Indices in tree order for navigation
}

impl App {
    pub async fn new(client: Client) -> Result<Self> {
        let mut app = Self {
            client,
            repo_idx: 0,
            branch_idx: 0,
            session_idx: 0,
            focus: Focus::Sidebar,
            repos: Vec::new(),
            worktrees: Vec::new(),
            available_branches: Vec::new(),
            add_worktree_idx: 0,
            sessions: Vec::new(),
            terminal_parser: Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))),
            session_parsers: HashMap::new(),
            active_session_id: None,
            is_interactive: false,
            terminal_stream: None,
            terminal_mode: TerminalMode::Normal,
            scroll_offset: 0,
            terminal_fullscreen: false,
            last_terminal_hash: 0,
            cached_terminal_lines: Vec::new(),
            cached_terminal_size: (0, 0),
            right_panel_view: RightPanelView::Terminal,
            diff_files: Vec::new(),
            diff_expanded: std::collections::HashSet::new(),
            diff_file_lines: std::collections::HashMap::new(),
            diff_cursor: 0,
            diff_scroll_offset: 0,
            diff_fullscreen: false,
            line_comments: Vec::new(),
            tree_view_enabled: true, // Enable tree view by default
            expanded_worktrees: std::collections::HashSet::new(),
            sidebar_cursor: 0,
            sessions_by_worktree: HashMap::new(),
            git_panel_enabled: true, // Enable git status panel by default
            git_status_files: Vec::new(),
            git_status_cursor: 0,
            expanded_git_sections: default_expanded_git_sections(),
            git_panel_scroll_offset: 0,
            pending_diff_file: None,
            terminal_cols: None,
            terminal_rows: None,
            should_quit: false,
            error_message: None,
            status_message: None,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            event_rx: None,
            prefix_mode: PrefixMode::None,
            dirty: DirtyFlags::default(),
            todo_items: Vec::new(),
            todo_cursor: 0,
            expanded_todos: std::collections::HashSet::new(),
            todo_scroll_offset: 0,
            todo_show_completed: true,
            todo_display_order: Vec::new(),
        };

        // Load initial data
        app.refresh_all().await?;

        // Load git status for current worktree
        let _ = app.load_git_status().await;

        // Subscribe to events (don't fail if subscription fails)
        app.subscribe_events().await;

        Ok(app)
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

        // Load repos
        self.repos = self.client.list_repos().await?;

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Clamp repo index
        if self.repos.is_empty() {
            self.repo_idx = 0;
            self.worktrees.clear();
            self.available_branches.clear();
            self.sessions.clear();
            return Ok(());
        }
        if self.repo_idx >= self.repos.len() {
            self.repo_idx = self.repos.len() - 1;
        }

        // Load branches for current repo
        self.refresh_branches().await?;

        Ok(())
    }

    /// Refresh worktrees for current repo
    pub async fn refresh_branches(&mut self) -> Result<()> {
        if let Some(repo) = self.repos.get(self.repo_idx) {
            let all_branches = self.client.list_worktrees(&repo.id).await?;
            // Split into worktrees (has path) and available branches (no path)
            self.worktrees = all_branches
                .iter()
                .filter(|b| !b.path.is_empty())
                .cloned()
                .collect();
            self.available_branches = all_branches
                .into_iter()
                .filter(|b| b.path.is_empty())
                .collect();
        } else {
            self.worktrees.clear();
            self.available_branches.clear();
        }

        // Clear worktree-related caches when refreshing
        self.sessions_by_worktree.clear();
        self.expanded_worktrees.clear();

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Clamp branch index
        if self.worktrees.is_empty() {
            self.branch_idx = 0;
            self.sessions.clear();
            return Ok(());
        }
        if self.branch_idx >= self.worktrees.len() {
            self.branch_idx = self.worktrees.len() - 1;
        }

        // Load sessions for current branch
        self.refresh_sessions().await?;

        // Load git status for current worktree
        let _ = self.load_git_status().await;

        Ok(())
    }

    /// Refresh sessions for current branch
    pub async fn refresh_sessions(&mut self) -> Result<()> {
        if let (Some(repo), Some(branch)) = (
            self.repos.get(self.repo_idx),
            self.worktrees.get(self.branch_idx),
        ) {
            self.sessions = self
                .client
                .list_sessions(Some(&repo.id), Some(&branch.branch))
                .await?;
        } else {
            self.sessions.clear();
        }

        // Clamp session index
        if !self.sessions.is_empty() && self.session_idx >= self.sessions.len() {
            self.session_idx = self.sessions.len() - 1;
        }

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Update active session for preview
        self.update_active_session().await;

        Ok(())
    }

    /// Update active session based on current selection
    async fn update_active_session(&mut self) {
        let new_session_id = self.sessions.get(self.session_idx).map(|s| s.id.clone());

        // If session changed, disconnect old stream and connect new one
        if self.active_session_id != new_session_id {
            self.disconnect_stream();

            // Save current parser to map if there's an active session
            if let Some(old_id) = &self.active_session_id {
                self.session_parsers
                    .insert(old_id.clone(), self.terminal_parser.clone());
            }

            // Get or create parser for new session
            if let Some(new_id) = &new_session_id {
                self.terminal_parser = self
                    .session_parsers
                    .entry(new_id.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                    .clone();
            } else {
                // No session selected, use a fresh parser
                self.terminal_parser = Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)));
            }

            self.scroll_offset = 0;
            self.active_session_id = new_session_id;

            // Auto-connect for preview if there's a session
            if self.active_session_id.is_some() {
                let _ = self.connect_stream().await;
            }
        }
    }

    /// Get current list length based on focus
    #[allow(dead_code)]
    pub fn current_list_len(&self) -> usize {
        match self.focus {
            Focus::Sidebar => self.sidebar_virtual_len(),
            Focus::GitStatus => self.git_status_virtual_len(),
            Focus::Branches => self.worktrees.len(),
            Focus::Sessions => self.sessions.len(),
            Focus::Terminal => 0,
            Focus::DiffFiles => self.diff_files.len(),
        }
    }

    /// Get current selection index based on focus
    #[allow(dead_code)]
    pub fn current_idx(&self) -> usize {
        match self.focus {
            Focus::Sidebar => self.sidebar_cursor,
            Focus::GitStatus => self.git_status_cursor,
            Focus::Branches => self.branch_idx,
            Focus::Sessions => self.session_idx,
            Focus::Terminal => 0,
            Focus::DiffFiles => self.diff_cursor,
        }
    }

    // ========== Tree view helpers ==========

    /// Calculate the virtual list length for tree view
    pub fn sidebar_virtual_len(&self) -> usize {
        let mut len = 0;
        for (i, _wt) in self.worktrees.iter().enumerate() {
            len += 1; // worktree itself
            if self.expanded_worktrees.contains(&i) {
                if let Some(sessions) = self.sessions_by_worktree.get(&i) {
                    len += sessions.len();
                }
            }
        }
        len
    }

    /// Get the current sidebar item at cursor position
    pub fn current_sidebar_item(&self) -> SidebarItem {
        let mut pos = 0;
        for (wt_idx, _wt) in self.worktrees.iter().enumerate() {
            if pos == self.sidebar_cursor {
                return SidebarItem::Worktree(wt_idx);
            }
            pos += 1;
            if self.expanded_worktrees.contains(&wt_idx) {
                if let Some(sessions) = self.sessions_by_worktree.get(&wt_idx) {
                    for (s_idx, _session) in sessions.iter().enumerate() {
                        if pos == self.sidebar_cursor {
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
        if let SidebarItem::Worktree(wt_idx) = self.current_sidebar_item() {
            if self.expanded_worktrees.contains(&wt_idx) {
                self.expanded_worktrees.remove(&wt_idx);
            } else {
                self.expanded_worktrees.insert(wt_idx);
                // Load sessions for this worktree if not loaded
                if !self.sessions_by_worktree.contains_key(&wt_idx) {
                    return Some(AsyncAction::LoadWorktreeSessions { wt_idx });
                }
            }
            self.dirty.sidebar = true;
        }
        None
    }

    /// Move cursor up in sidebar tree view
    pub fn sidebar_move_up(&mut self) -> Option<AsyncAction> {
        if self.sidebar_cursor > 0 {
            self.sidebar_cursor -= 1;
            self.dirty.sidebar = true;
            if self.update_selection_from_sidebar() {
                // Worktree changed, refresh git status
                return Some(AsyncAction::LoadGitStatus);
            }
        }
        None
    }

    /// Move cursor down in sidebar tree view
    pub fn sidebar_move_down(&mut self) -> Option<AsyncAction> {
        let max_cursor = self.sidebar_virtual_len().saturating_sub(1);
        if self.sidebar_cursor < max_cursor {
            self.sidebar_cursor += 1;
            self.dirty.sidebar = true;
            if self.update_selection_from_sidebar() {
                // Worktree changed, refresh git status
                return Some(AsyncAction::LoadGitStatus);
            }
        }
        None
    }

    /// Toggle between tree view and legacy split view
    pub fn toggle_tree_view(&mut self) {
        self.tree_view_enabled = !self.tree_view_enabled;
        self.dirty.sidebar = true;

        // Update focus based on mode
        if self.tree_view_enabled {
            // Switch to tree view: change Focus::Branches/Sessions to Focus::Sidebar
            if self.focus == Focus::Branches || self.focus == Focus::Sessions {
                self.focus = Focus::Sidebar;
            }
        } else {
            // Switch to legacy view: change Focus::Sidebar to Focus::Branches
            if self.focus == Focus::Sidebar {
                self.focus = Focus::Branches;
            }
        }

        self.status_message = Some(if self.tree_view_enabled {
            "Tree view enabled (T to toggle)".to_string()
        } else {
            "Legacy view enabled (T to toggle)".to_string()
        });
    }

    /// Update branch_idx and session_idx based on sidebar cursor
    /// Returns true if the worktree changed (needs git status refresh)
    fn update_selection_from_sidebar(&mut self) -> bool {
        let old_branch_idx = self.branch_idx;

        match self.current_sidebar_item() {
            SidebarItem::Worktree(wt_idx) => {
                self.branch_idx = wt_idx;
                self.session_idx = 0;
                // Don't clear active session when navigating to worktree
                // Keep showing the current terminal content
            }
            SidebarItem::Session(wt_idx, s_idx) => {
                self.branch_idx = wt_idx;
                self.session_idx = s_idx;
                // Get session id first to avoid borrow issues
                let session_id = self
                    .sessions_by_worktree
                    .get(&wt_idx)
                    .and_then(|sessions| sessions.get(s_idx))
                    .map(|s| s.id.clone());

                if let Some(new_id) = session_id {
                    if self.active_session_id.as_ref() != Some(&new_id) {
                        self.disconnect_stream();

                        // Save current parser if there was an active session
                        if let Some(old_id) = &self.active_session_id {
                            self.session_parsers
                                .insert(old_id.clone(), self.terminal_parser.clone());
                        }

                        // Get or create parser for new session
                        self.terminal_parser = self
                            .session_parsers
                            .entry(new_id.clone())
                            .or_insert_with(|| {
                                Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)))
                            })
                            .clone();

                        self.active_session_id = Some(new_id);
                        self.scroll_offset = 0;
                        self.dirty.terminal = true;
                    }
                }
            }
            SidebarItem::None => {}
        }

        // Return true if worktree changed
        self.branch_idx != old_branch_idx
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
            Focus::Branches => {
                if self.branch_idx > 0 {
                    self.branch_idx -= 1;
                    self.dirty.sidebar = true;
                    return Some(AsyncAction::RefreshSessions);
                }
            }
            Focus::Sessions => {
                if self.session_idx > 0 {
                    self.session_idx -= 1;
                    self.dirty.sidebar = true;
                    self.dirty.terminal = true;
                    self.update_active_session_sync();
                }
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
            Focus::Branches => {
                if !self.worktrees.is_empty() && self.branch_idx < self.worktrees.len() - 1 {
                    self.branch_idx += 1;
                    self.dirty.sidebar = true;
                    return Some(AsyncAction::RefreshSessions);
                }
            }
            Focus::Sessions => {
                if !self.sessions.is_empty() && self.session_idx < self.sessions.len() - 1 {
                    self.session_idx += 1;
                    self.dirty.sidebar = true;
                    self.dirty.terminal = true;
                    self.update_active_session_sync();
                }
            }
            Focus::Terminal => {}
            Focus::DiffFiles => {
                self.diff_move_down();
            }
        }
        None
    }

    /// Switch to repo by index (sync version)
    pub fn switch_repo_sync(&mut self, idx: usize) -> Option<AsyncAction> {
        if idx < self.repos.len() {
            self.repo_idx = idx;
            self.branch_idx = 0;
            self.session_idx = 0;
            self.dirty.sidebar = true;
            return Some(AsyncAction::RefreshBranches);
        }
        None
    }

    /// Update active session state (sync version - no stream connection)
    fn update_active_session_sync(&mut self) {
        let new_session_id = self.sessions.get(self.session_idx).map(|s| s.id.clone());

        if self.active_session_id != new_session_id {
            self.disconnect_stream();

            // Save current parser to map if there's an active session
            if let Some(old_id) = &self.active_session_id {
                self.session_parsers
                    .insert(old_id.clone(), self.terminal_parser.clone());
            }

            // Get or create parser for new session
            if let Some(new_id) = &new_session_id {
                self.terminal_parser = self
                    .session_parsers
                    .entry(new_id.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                    .clone();
            } else {
                self.terminal_parser = Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)));
            }

            self.scroll_offset = 0;
            self.active_session_id = new_session_id;
            self.dirty.terminal = true;
            // Note: Stream connection is deferred - will happen when user enters terminal
        }
    }

    /// Toggle focus between Branches and Sessions
    #[allow(dead_code)]
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Sidebar => Focus::Sidebar,   // Stay in sidebar
            Focus::GitStatus => Focus::Sidebar, // Go back to sidebar
            Focus::Branches => Focus::Sessions,
            Focus::Sessions => Focus::Branches,
            Focus::Terminal => Focus::Sessions,
            Focus::DiffFiles => Focus::Branches,
        };
    }

    /// Enter terminal Insert mode (from Sessions)
    pub async fn enter_terminal(&mut self) -> Result<()> {
        if self.active_session_id.is_some() {
            self.focus = Focus::Terminal;
            self.terminal_mode = TerminalMode::Insert;
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
        self.terminal_mode = TerminalMode::Insert;
        self.scroll_to_bottom();
    }

    /// Exit to Normal mode (from Insert mode)
    #[allow(dead_code)]
    pub fn exit_to_normal_mode(&mut self) {
        self.terminal_mode = TerminalMode::Normal;
        deactivate_ime();
    }

    /// Exit terminal mode (back to sidebar)
    pub fn exit_terminal(&mut self) {
        if self.terminal_fullscreen {
            self.terminal_fullscreen = false;
        } else {
            // Return to appropriate sidebar focus based on tree view mode
            self.focus = if self.tree_view_enabled {
                Focus::Sidebar
            } else {
                Focus::Sessions
            };
            self.terminal_mode = TerminalMode::Normal;
            self.is_interactive = false;
        }
    }

    /// Switch to or create shell session for current worktree (Ctrl+`)
    pub async fn switch_to_shell_session(&mut self) -> Result<()> {
        const SHELL_SESSION_NAME: &str = "__shell__";

        // Get current worktree
        let current_worktree = match self.worktrees.get(self.branch_idx) {
            Some(wt) => wt.clone(),
            None => {
                self.status_message = Some("No worktree selected".to_string());
                return Ok(());
            }
        };

        // Find shell session in current worktree
        let shell_session = self
            .sessions
            .iter()
            .find(|s| {
                s.name == SHELL_SESSION_NAME
                    && s.branch == current_worktree.branch
                    && s.is_shell == Some(true)
            })
            .cloned();

        if let Some(session) = shell_session {
            // Shell session exists, switch to it
            let new_id = session.id.clone();

            // Disconnect current stream
            self.disconnect_stream();

            // Save current parser if there's an active session
            if let Some(old_id) = &self.active_session_id {
                self.session_parsers
                    .insert(old_id.clone(), self.terminal_parser.clone());
            }

            // Get or create parser for shell session
            self.terminal_parser = self
                .session_parsers
                .entry(new_id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                .clone();

            self.scroll_offset = 0;
            self.active_session_id = Some(new_id);
            self.dirty.terminal = true;

            self.enter_terminal().await?;
            self.status_message = Some("Switched to shell session".to_string());
        } else {
            // Create new shell session
            let repo = match self.repos.get(self.repo_idx) {
                Some(r) => r.clone(),
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
                    // Refresh sessions list (both legacy and tree view)
                    self.refresh_sessions().await?;
                    // Also refresh tree view sessions for current worktree
                    if self.tree_view_enabled {
                        self.load_worktree_sessions(self.branch_idx).await?;
                    }

                    let new_id = session.id;

                    // Disconnect current stream
                    self.disconnect_stream();

                    // Save current parser if there's an active session
                    if let Some(old_id) = &self.active_session_id {
                        self.session_parsers
                            .insert(old_id.clone(), self.terminal_parser.clone());
                    }

                    // Create parser for new shell session
                    self.terminal_parser = Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)));
                    self.session_parsers
                        .insert(new_id.clone(), self.terminal_parser.clone());

                    self.scroll_offset = 0;
                    self.active_session_id = Some(new_id);
                    self.dirty.terminal = true;

                    self.enter_terminal().await?;
                    self.status_message = Some("Created shell session".to_string());
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to create shell session: {}", e));
                }
            }
        }

        Ok(())
    }

    /// Toggle fullscreen mode
    pub fn toggle_fullscreen(&mut self) {
        self.terminal_fullscreen = !self.terminal_fullscreen;
    }

    /// Scroll up (older content)
    pub fn scroll_up(&mut self, lines: usize) {
        if let Ok(mut parser) = self.terminal_parser.lock() {
            let current = parser.screen().scrollback();
            let new_offset = current + lines;
            parser.screen_mut().set_scrollback(new_offset);
            self.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll down (newer content)
    pub fn scroll_down(&mut self, lines: usize) {
        if let Ok(mut parser) = self.terminal_parser.lock() {
            let current = parser.screen().scrollback();
            let new_offset = current.saturating_sub(lines);
            parser.screen_mut().set_scrollback(new_offset);
            self.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        if let Ok(mut parser) = self.terminal_parser.lock() {
            parser.screen_mut().set_scrollback(usize::MAX);
            self.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        if let Ok(mut parser) = self.terminal_parser.lock() {
            parser.screen_mut().set_scrollback(0);
            self.scroll_offset = 0;
        }
    }

    /// Enter interactive mode (deprecated, use enter_terminal)
    #[allow(dead_code)]
    pub async fn enter_interactive(&mut self) -> Result<()> {
        self.enter_terminal().await
    }

    /// Exit interactive mode
    #[allow(dead_code)]
    pub fn exit_interactive(&mut self) {
        self.is_interactive = false;
        self.focus = if self.tree_view_enabled {
            Focus::Sidebar
        } else {
            Focus::Sessions
        };
    }

    /// Create new session and enter interactive mode
    pub async fn create_new(&mut self) -> Result<()> {
        match self.focus {
            Focus::Sidebar => {
                // In tree view: create session for currently selected worktree
                if let (Some(repo), Some(branch)) = (
                    self.repos.get(self.repo_idx).cloned(),
                    self.worktrees.get(self.branch_idx).cloned(),
                ) {
                    match self
                        .client
                        .create_session(&repo.id, &branch.branch, None, None)
                        .await
                    {
                        Ok(session) => {
                            // Refresh sessions for this worktree
                            self.load_worktree_sessions(self.branch_idx).await?;
                            // Expand worktree
                            self.expanded_worktrees.insert(self.branch_idx);
                            // Set active session before entering terminal
                            self.active_session_id = Some(session.id.clone());
                            self.enter_terminal().await?;
                        }
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                        }
                    }
                }
            }
            Focus::Branches => {
                // Enter input mode for new branch name
                self.input_mode = InputMode::NewBranch;
                self.input_buffer.clear();
                self.status_message = Some("Enter branch name:".to_string());
            }
            Focus::Sessions => {
                // Create new session for current branch
                if let (Some(repo), Some(branch)) = (
                    self.repos.get(self.repo_idx).cloned(),
                    self.worktrees.get(self.branch_idx).cloned(),
                ) {
                    match self
                        .client
                        .create_session(&repo.id, &branch.branch, None, None)
                        .await
                    {
                        Ok(session) => {
                            self.refresh_sessions().await?;
                            // Find and select the new session
                            if let Some(idx) = self.sessions.iter().position(|s| s.id == session.id)
                            {
                                self.session_idx = idx;
                                self.update_active_session().await;
                                self.enter_terminal().await?;
                            }
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

        let branch_name = self.input_buffer.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status_message = None;

        if branch_name.is_empty() {
            self.error_message = Some("Branch name cannot be empty".to_string());
            return Ok(());
        }

        // Create session (will auto-create worktree if needed)
        if let Some(repo) = self.repos.get(self.repo_idx).cloned() {
            match self
                .client
                .create_session(&repo.id, &branch_name, None, None)
                .await
            {
                Ok(session) => {
                    self.refresh_branches().await?;
                    // Find the branch and session
                    if let Some(b_idx) = self.worktrees.iter().position(|b| b.branch == branch_name)
                    {
                        self.branch_idx = b_idx;
                        self.refresh_sessions().await?;
                        if let Some(s_idx) = self.sessions.iter().position(|s| s.id == session.id) {
                            self.session_idx = s_idx;
                            self.update_active_session().await;
                            // Also load sessions for tree view
                            self.load_worktree_sessions(b_idx).await?;
                            self.expanded_worktrees.insert(b_idx);
                            // Return to appropriate focus before entering terminal
                            self.focus = if self.tree_view_enabled {
                                Focus::Sidebar
                            } else {
                                Focus::Sessions
                            };
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

    /// Cancel input mode
    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status_message = None;
    }

    /// Start add worktree mode
    pub fn start_add_worktree(&mut self) {
        // Get current selected branch as base (None = use HEAD)
        let base_branch = self
            .worktrees
            .get(self.branch_idx)
            .map(|w| w.branch.clone());

        self.input_mode = InputMode::AddWorktree { base_branch };
        self.input_buffer.clear();
        self.add_worktree_idx = 0;
    }

    /// Start rename session mode
    pub fn start_rename_session(&mut self) {
        if let Some(session) = self.sessions.get(self.session_idx) {
            self.input_mode = InputMode::RenameSession {
                session_id: session.id.clone(),
            };
            self.input_buffer = session.name.clone();
        }
    }

    /// Submit rename session
    pub async fn submit_rename_session(&mut self) -> Result<()> {
        let session_id = match &self.input_mode {
            InputMode::RenameSession { session_id } => session_id.clone(),
            _ => return Ok(()),
        };

        let new_name = self.input_buffer.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();

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
                self.load_worktree_sessions(self.branch_idx).await?;
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
        let (branch_name, use_base) = if !self.input_buffer.is_empty() {
            // Creating new branch - use base_branch
            (self.input_buffer.trim().to_string(), true)
        } else if let Some(branch) = self.available_branches.get(self.add_worktree_idx) {
            // Selecting existing branch - no need for base
            (branch.branch.clone(), false)
        } else {
            self.cancel_input();
            return Ok(());
        };

        let repo_id = match self.repos.get(self.repo_idx) {
            Some(repo) => repo.id.clone(),
            None => {
                self.cancel_input();
                return Ok(());
            }
        };

        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();

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
                if let Some(idx) = self.worktrees.iter().position(|w| w.branch == branch_name) {
                    self.branch_idx = idx;
                    self.sidebar_cursor = idx;
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
            if self.active_session_id.as_ref() == Some(&session.id) {
                self.disconnect_stream();
            }
            self.client.destroy_session(&session.id).await?;
        }

        // Now proceed to delete worktree (show confirmation for worktree deletion)
        self.input_mode = InputMode::ConfirmDelete(DeleteTarget::Worktree { repo_id, branch });

        // Refresh sessions to update the UI
        self.refresh_sessions().await?;
        // Also refresh worktree sessions for tree view
        self.load_worktree_sessions(self.branch_idx).await?;

        Ok(())
    }

    /// Confirm and delete branch (called after worktree deletion)
    pub async fn confirm_delete_branch(&mut self) -> Result<()> {
        let branch_name = match &self.input_mode {
            InputMode::ConfirmDeleteBranch(b) => b.clone(),
            _ => return Ok(()),
        };

        self.input_mode = InputMode::Normal;

        // Get repo_id
        let repo_id = match self.repos.get(self.repo_idx) {
            Some(repo) => repo.id.clone(),
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
        match self.focus {
            Focus::Sidebar => {
                // In tree view: delete based on current selection
                match self.current_sidebar_item() {
                    SidebarItem::Worktree(wt_idx) => {
                        if let (Some(repo), Some(wt)) = (
                            self.repos.get(self.repo_idx).cloned(),
                            self.worktrees.get(wt_idx).cloned(),
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
                        if let Some(sessions) = self.sessions_by_worktree.get(&wt_idx) {
                            if let Some(session) = sessions.get(s_idx) {
                                self.input_mode = InputMode::ConfirmDelete(DeleteTarget::Session {
                                    session_id: session.id.clone(),
                                    name: session.name.clone(),
                                });
                            }
                        }
                    }
                    SidebarItem::None => {}
                }
            }
            Focus::Branches => {
                if let (Some(repo), Some(wt)) = (
                    self.repos.get(self.repo_idx).cloned(),
                    self.worktrees.get(self.branch_idx).cloned(),
                ) {
                    if wt.is_main {
                        self.error_message = Some("Cannot remove main worktree".to_string());
                    } else if wt.path.is_empty() {
                        self.error_message = Some("No worktree to remove".to_string());
                    } else if wt.session_count > 0 {
                        // Worktree has sessions, ask to delete them first
                        self.input_mode = InputMode::ConfirmDeleteWorktreeSessions {
                            repo_id: repo.id,
                            branch: wt.branch,
                            session_count: wt.session_count,
                        };
                    } else {
                        self.input_mode = InputMode::ConfirmDelete(DeleteTarget::Worktree {
                            repo_id: repo.id,
                            branch: wt.branch,
                        });
                    }
                }
            }
            Focus::Sessions => {
                if let Some(session) = self.sessions.get(self.session_idx).cloned() {
                    self.input_mode = InputMode::ConfirmDelete(DeleteTarget::Session {
                        session_id: session.id,
                        name: session.name,
                    });
                }
            }
            Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => {}
        }
    }

    /// Confirm and execute deletion
    pub async fn confirm_delete(&mut self) -> Result<()> {
        let target = match &self.input_mode {
            InputMode::ConfirmDelete(t) => t.clone(),
            _ => return Ok(()),
        };

        self.input_mode = InputMode::Normal;

        match target {
            DeleteTarget::Worktree { repo_id, branch } => {
                match self.client.remove_worktree(&repo_id, &branch).await {
                    Ok(_) => {
                        // After removing worktree, ask if user wants to delete branch too
                        self.input_mode = InputMode::ConfirmDeleteBranch(branch);
                        self.refresh_branches().await?;
                    }
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                    }
                }
            }
            DeleteTarget::Session { session_id, name } => {
                // Disconnect if this is the active session
                if self.active_session_id.as_ref() == Some(&session_id) {
                    self.disconnect_stream();
                }

                match self.client.destroy_session(&session_id).await {
                    Ok(_) => {
                        self.status_message = Some(format!("Destroyed session: {}", name));
                        self.refresh_sessions().await?;
                        // Also refresh worktree sessions for tree view
                        self.load_worktree_sessions(self.branch_idx).await?;
                    }
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                    }
                }
            }
        }

        Ok(())
    }

    /// Connect to session stream for preview/interaction
    pub async fn connect_stream(&mut self) -> Result<()> {
        let session_id = match &self.active_session_id {
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
        if let Ok(mut parser) = self.terminal_parser.lock() {
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

    /// Poll terminal output (non-blocking)
    /// Note: Not used with tokio::select! architecture, kept for potential fallback
    #[allow(dead_code)]
    pub fn poll_terminal_output(&mut self) {
        if let Some(stream) = &mut self.terminal_stream {
            // Try to receive without blocking
            while let Ok(data) = stream.output_rx.try_recv() {
                if let Ok(mut parser) = self.terminal_parser.lock() {
                    parser.process(&data);
                }
            }
        }
    }

    /// Poll daemon events (non-blocking)
    /// Returns true if any event was processed, or if resubscription is needed
    /// Note: Not used with tokio::select! architecture, kept for potential fallback
    #[allow(dead_code)]
    pub fn poll_events(&mut self) -> bool {
        let mut processed = false;
        let mut channel_closed = false;

        // Collect events first to avoid borrow issues
        let mut events = Vec::new();
        if let Some(rx) = &mut self.event_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        // Channel closed, need to resubscribe
                        channel_closed = true;
                        break;
                    }
                }
            }
        }

        // Mark channel as closed so we can resubscribe
        if channel_closed {
            warn!("Event channel disconnected, will attempt resubscription");
            self.event_rx = None;
        }

        // Process collected events
        for event in events {
            self.handle_daemon_event(event);
            processed = true;
        }
        processed
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
    fn handle_daemon_event(&mut self, event: DaemonEvent) -> bool {
        match event.event {
            Some(daemon_event::Event::SessionCreated(e)) => {
                debug!(
                    "Event: SessionCreated {:?}",
                    e.session.as_ref().map(|s| &s.id)
                );
                if let Some(session) = e.session {
                    // Only add if it matches current repo/branch filter
                    if let (Some(repo), Some(branch)) = (
                        self.repos.get(self.repo_idx),
                        self.worktrees.get(self.branch_idx),
                    ) {
                        if session.repo_id == repo.id && session.branch == branch.branch {
                            self.sessions.push(session);
                            return true; // Session list changed
                        }
                    }
                }
                false
            }
            Some(daemon_event::Event::SessionDestroyed(e)) => {
                debug!("Event: SessionDestroyed {}", e.session_id);
                let old_len = self.sessions.len();
                // Remove session from list
                self.sessions.retain(|s| s.id != e.session_id);
                // Clamp session index
                if !self.sessions.is_empty() && self.session_idx >= self.sessions.len() {
                    self.session_idx = self.sessions.len() - 1;
                }
                self.sessions.len() != old_len // Only redraw if session was actually removed
            }
            Some(daemon_event::Event::SessionNameUpdated(e)) => {
                debug!(
                    "Event: SessionNameUpdated {} -> {}",
                    e.session_id, e.new_name
                );
                // Update session name in list
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == e.session_id) {
                    if session.name != e.new_name {
                        session.name = e.new_name;
                        return true; // Name changed
                    }
                }
                false
            }
            Some(daemon_event::Event::SessionStatusChanged(e)) => {
                debug!(
                    "Event: SessionStatusChanged {} -> {}",
                    e.session_id, e.new_status
                );
                // Update session status in list - only redraw if status actually changed
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == e.session_id) {
                    if session.status != e.new_status {
                        session.status = e.new_status;
                        return true; // Status changed
                    }
                }
                false
            }
            Some(daemon_event::Event::WorktreeAdded(e)) => {
                debug!(
                    "Event: WorktreeAdded {:?}",
                    e.worktree.as_ref().map(|w| &w.branch)
                );
                if let Some(worktree) = e.worktree {
                    // Only add if it matches current repo
                    if let Some(repo) = self.repos.get(self.repo_idx) {
                        if worktree.repo_id == repo.id {
                            // Check if worktree already exists to avoid duplicates
                            if !self.worktrees.iter().any(|w| w.branch == worktree.branch) {
                                self.worktrees.push(worktree);
                                self.dirty.sidebar = true;
                                return true;
                            }
                        }
                    }
                }
                false
            }
            Some(daemon_event::Event::WorktreeRemoved(e)) => {
                debug!("Event: WorktreeRemoved {} {}", e.repo_id, e.branch);
                // Remove worktree from list if it matches current repo
                if let Some(repo) = self.repos.get(self.repo_idx) {
                    if e.repo_id == repo.id {
                        let old_len = self.worktrees.len();
                        self.worktrees.retain(|w| w.branch != e.branch);
                        // Clamp branch index
                        if !self.worktrees.is_empty() && self.branch_idx >= self.worktrees.len() {
                            self.branch_idx = self.worktrees.len() - 1;
                        }
                        if self.worktrees.len() != old_len {
                            self.dirty.sidebar = true;
                            return true;
                        }
                    }
                }
                false
            }
            None => false,
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
            AsyncAction::ConfirmDelete => {
                self.confirm_delete().await?;
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
                let _ = self.load_worktree_sessions(self.branch_idx).await;
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
        if let (Some(repo), Some(worktree)) =
            (self.repos.get(self.repo_idx), self.worktrees.get(wt_idx))
        {
            let sessions = self
                .client
                .list_sessions(Some(&repo.id), Some(&worktree.branch))
                .await?;
            self.sessions_by_worktree.insert(wt_idx, sessions);
            self.dirty.sidebar = true;
        }
        Ok(())
    }

    // ========== Git Status Methods ==========

    /// Load git status for current worktree
    pub async fn load_git_status(&mut self) -> Result<()> {
        if let (Some(repo), Some(worktree)) = (
            self.repos.get(self.repo_idx).cloned(),
            self.worktrees.get(self.branch_idx).cloned(),
        ) {
            let response = self
                .client
                .get_git_status(&repo.id, &worktree.branch)
                .await?;

            // Convert proto files to local representation
            self.git_status_files.clear();

            for f in response.staged {
                self.git_status_files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Staged,
                });
            }
            for f in response.unstaged {
                self.git_status_files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Unstaged,
                });
            }
            for f in response.untracked {
                self.git_status_files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Untracked,
                });
            }

            self.git_status_cursor = 0;
            self.dirty.sidebar = true;

            // Also load comments for this branch
            match self
                .client
                .list_line_comments(&repo.id, &worktree.branch, None)
                .await
            {
                Ok(comments) => {
                    self.line_comments = comments;
                }
                Err(e) => {
                    tracing::warn!("Failed to load comments: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Stage a single file
    pub async fn stage_file(&mut self, file_path: &str) -> Result<()> {
        if let (Some(repo), Some(worktree)) = (
            self.repos.get(self.repo_idx),
            self.worktrees.get(self.branch_idx),
        ) {
            self.client
                .stage_file(&repo.id, &worktree.branch, file_path)
                .await?;
            // Reload git status after staging
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Unstage a single file
    pub async fn unstage_file(&mut self, file_path: &str) -> Result<()> {
        if let (Some(repo), Some(worktree)) = (
            self.repos.get(self.repo_idx),
            self.worktrees.get(self.branch_idx),
        ) {
            self.client
                .unstage_file(&repo.id, &worktree.branch, file_path)
                .await?;
            // Reload git status after unstaging
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Stage all files
    pub async fn stage_all(&mut self) -> Result<()> {
        if let (Some(repo), Some(worktree)) = (
            self.repos.get(self.repo_idx),
            self.worktrees.get(self.branch_idx),
        ) {
            self.client.stage_all(&repo.id, &worktree.branch).await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Unstage all files
    pub async fn unstage_all(&mut self) -> Result<()> {
        if let (Some(repo), Some(worktree)) = (
            self.repos.get(self.repo_idx),
            self.worktrees.get(self.branch_idx),
        ) {
            self.client.unstage_all(&repo.id, &worktree.branch).await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    // ============ TODO Operations ============

    /// Load TODO items for current repository
    pub async fn load_todos(&mut self) -> Result<()> {
        if let Some(repo) = self.repos.get(self.repo_idx) {
            self.todo_items = self
                .client
                .list_todos(&repo.id, self.todo_show_completed)
                .await?;
            self.rebuild_todo_display_order();
        }
        Ok(())
    }

    /// Rebuild display order for TODO items (tree structure)
    pub fn rebuild_todo_display_order(&mut self) {
        use std::collections::HashMap;

        // Build parent-to-children mapping
        let mut items_by_parent: HashMap<Option<String>, Vec<usize>> = HashMap::new();
        for (i, item) in self.todo_items.iter().enumerate() {
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

        self.todo_display_order.clear();
        build_order(
            &self.todo_items,
            &items_by_parent,
            None,
            &mut self.todo_display_order,
        );
    }

    /// Create a new TODO item
    pub async fn create_todo(
        &mut self,
        title: String,
        description: Option<String>,
        parent_id: Option<String>,
    ) -> Result<()> {
        if let Some(repo) = self.repos.get(self.repo_idx) {
            self.client
                .create_todo(&repo.id, title, description, parent_id)
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

    /// Calculate virtual list length for git status panel
    pub fn git_status_virtual_len(&self) -> usize {
        let mut len = 0;

        // Count staged section
        let staged_count = self
            .git_status_files
            .iter()
            .filter(|f| f.section == GitSection::Staged)
            .count();
        if staged_count > 0 {
            len += 1; // Section header
            if self.expanded_git_sections.contains(&GitSection::Staged) {
                len += staged_count;
            }
        }

        // Count unstaged section
        let unstaged_count = self
            .git_status_files
            .iter()
            .filter(|f| f.section == GitSection::Unstaged)
            .count();
        if unstaged_count > 0 {
            len += 1; // Section header
            if self.expanded_git_sections.contains(&GitSection::Unstaged) {
                len += unstaged_count;
            }
        }

        // Count untracked section
        let untracked_count = self
            .git_status_files
            .iter()
            .filter(|f| f.section == GitSection::Untracked)
            .count();
        if untracked_count > 0 {
            len += 1; // Section header
            if self.expanded_git_sections.contains(&GitSection::Untracked) {
                len += untracked_count;
            }
        }

        len
    }

    /// Get current git panel item at cursor position
    pub fn current_git_panel_item(&self) -> GitPanelItem {
        let mut pos = 0;
        let sections = [
            GitSection::Staged,
            GitSection::Unstaged,
            GitSection::Untracked,
        ];

        for section in sections {
            let files: Vec<_> = self
                .git_status_files
                .iter()
                .enumerate()
                .filter(|(_, f)| f.section == section)
                .collect();

            if files.is_empty() {
                continue;
            }

            // Section header
            if pos == self.git_status_cursor {
                return GitPanelItem::Section(section);
            }
            pos += 1;

            // Files in section (if expanded)
            if self.expanded_git_sections.contains(&section) {
                for (file_idx, _) in files {
                    if pos == self.git_status_cursor {
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
            if self.expanded_git_sections.contains(&section) {
                self.expanded_git_sections.remove(&section);
            } else {
                self.expanded_git_sections.insert(section);
            }
            self.dirty.sidebar = true;
        }
    }

    /// Move cursor up in git status panel
    pub fn git_status_move_up(&mut self) {
        if self.git_status_cursor > 0 {
            self.git_status_cursor -= 1;
            self.dirty.sidebar = true;
        }
    }

    /// Move cursor down in git status panel
    pub fn git_status_move_down(&mut self) {
        let max_cursor = self.git_status_virtual_len().saturating_sub(1);
        if self.git_status_cursor < max_cursor {
            self.git_status_cursor += 1;
            self.dirty.sidebar = true;
        }
    }

    /// Get file path of currently selected git status file
    pub fn current_git_file_path(&self) -> Option<String> {
        if let GitPanelItem::File(idx) = self.current_git_panel_item() {
            self.git_status_files.get(idx).map(|f| f.path.clone())
        } else {
            None
        }
    }

    /// Check if current git item is staged
    pub fn is_current_git_item_staged(&self) -> bool {
        if let GitPanelItem::File(idx) = self.current_git_panel_item() {
            self.git_status_files
                .get(idx)
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
        self.terminal_cols = Some(cols);
        self.terminal_rows = Some(rows);

        // Calculate inner area (same as connect_stream)
        let main_height = rows.saturating_sub(6);
        let terminal_width = (cols as f32 * 0.75) as u16;
        let inner_rows = main_height.saturating_sub(2);
        let inner_cols = terminal_width.saturating_sub(2);

        // Resize parser
        if let Ok(mut parser) = self.terminal_parser.lock() {
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

    /// Calculate a hash of visible terminal content for change detection
    /// Returns true if terminal content has changed since last call
    pub fn update_terminal_hash(&mut self) -> bool {
        let new_hash = self.calculate_terminal_hash();
        if new_hash != self.last_terminal_hash {
            self.last_terminal_hash = new_hash;
            true
        } else {
            false
        }
    }

    /// Calculate hash of current terminal content
    fn calculate_terminal_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();

        if let Ok(parser) = self.terminal_parser.lock() {
            let screen = parser.screen();
            // Hash the terminal contents as a simple string
            // This is lightweight since we only need to detect changes
            screen.contents().hash(&mut hasher);
            // Also hash cursor position as it affects rendering
            let (cursor_row, cursor_col) = screen.cursor_position();
            cursor_row.hash(&mut hasher);
            cursor_col.hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Get terminal lines for rendering (uses cache when available)
    pub fn get_terminal_lines(&self, height: u16, width: u16) -> Vec<ratatui::text::Line<'static>> {
        // Use cached lines if available and size matches
        if self.cached_terminal_size == (height, width) && !self.cached_terminal_lines.is_empty() {
            return self.cached_terminal_lines.clone();
        }

        // Generate lines from vt100 parser
        self.generate_terminal_lines(height, width)
    }

    /// Generate terminal lines from vt100 parser (internal, always regenerates)
    fn generate_terminal_lines(
        &self,
        height: u16,
        width: u16,
    ) -> Vec<ratatui::text::Line<'static>> {
        let mut lines = Vec::new();

        if let Ok(parser) = self.terminal_parser.lock() {
            let screen = parser.screen();

            for row in 0..height {
                let mut spans: Vec<ratatui::text::Span<'static>> = Vec::new();
                let mut col = 0u16;

                while col < width {
                    if let Some(cell) = screen.cell(row, col) {
                        let ch = cell.contents();
                        if ch.is_empty() {
                            spans.push(ratatui::text::Span::raw(" "));
                            col += 1;
                        } else {
                            // Build style from cell attributes
                            let mut style = ratatui::style::Style::default();

                            // Apply foreground color
                            let fg = cell.fgcolor();
                            if fg != vt100::Color::Default {
                                style = style.fg(vt100_color_to_ratatui(fg));
                            }

                            // Apply background color
                            let bg = cell.bgcolor();
                            if bg != vt100::Color::Default {
                                style = style.bg(vt100_color_to_ratatui(bg));
                            }

                            // Apply attributes
                            if cell.bold() {
                                style = style.add_modifier(ratatui::style::Modifier::BOLD);
                            }
                            if cell.italic() {
                                style = style.add_modifier(ratatui::style::Modifier::ITALIC);
                            }
                            if cell.underline() {
                                style = style.add_modifier(ratatui::style::Modifier::UNDERLINED);
                            }
                            if cell.inverse() {
                                style = style.add_modifier(ratatui::style::Modifier::REVERSED);
                            }

                            spans.push(ratatui::text::Span::styled(ch.to_owned(), style));

                            // Skip columns for wide characters
                            use unicode_width::UnicodeWidthStr;
                            let w = UnicodeWidthStr::width(ch);
                            col += w.max(1) as u16;
                        }
                    } else {
                        spans.push(ratatui::text::Span::raw(" "));
                        col += 1;
                    }
                }

                lines.push(ratatui::text::Line::from(spans));
            }
        }

        lines
    }

    /// Update terminal line cache (call before rendering when content changed)
    #[allow(dead_code)]
    pub fn update_terminal_cache(&mut self, height: u16, width: u16) {
        self.cached_terminal_lines = self.generate_terminal_lines(height, width);
        self.cached_terminal_size = (height, width);
    }

    /// Invalidate terminal cache (call when session changes)
    #[allow(dead_code)]
    pub fn invalidate_terminal_cache(&mut self) {
        self.cached_terminal_lines.clear();
        self.cached_terminal_size = (0, 0);
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
        // Return to appropriate sidebar focus
        self.focus = if self.tree_view_enabled {
            Focus::Sidebar
        } else {
            Focus::Sessions
        };
        self.diff_files.clear();
        self.diff_expanded.clear();
        self.diff_file_lines.clear();
        self.diff_cursor = 0;
        self.diff_scroll_offset = 0;
    }

    /// Load diff files for current worktree
    pub async fn load_diff_files(&mut self) -> Result<()> {
        if let (Some(repo), Some(branch)) = (
            self.repos.get(self.repo_idx).cloned(),
            self.worktrees.get(self.branch_idx).cloned(),
        ) {
            match self.client.get_diff_files(&repo.id, &branch.branch).await {
                Ok(files) => {
                    self.diff_files = files;
                    self.diff_expanded.clear();
                    self.diff_file_lines.clear();
                    self.diff_cursor = 0;
                    self.diff_scroll_offset = 0;

                    // If there's a pending file to expand, find and expand it
                    if let Some(pending_file) = self.pending_diff_file.take() {
                        if let Some(idx) =
                            self.diff_files.iter().position(|f| f.path == pending_file)
                        {
                            self.diff_cursor = idx;
                            self.diff_expanded.insert(idx);
                            // Load the file's diff content
                            self.load_file_diff().await?;
                        }
                    }
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to load diff: {}", e));
                }
            }

            // Also load comments for this branch
            match self
                .client
                .list_line_comments(&repo.id, &branch.branch, None)
                .await
            {
                Ok(comments) => {
                    self.line_comments = comments;
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
        let file_idx = self
            .diff_expanded
            .iter()
            .find(|&&idx| !self.diff_file_lines.contains_key(&idx))
            .copied();

        if let Some(file_idx) = file_idx {
            if let (Some(repo), Some(branch), Some(file)) = (
                self.repos.get(self.repo_idx).cloned(),
                self.worktrees.get(self.branch_idx).cloned(),
                self.diff_files.get(file_idx).cloned(),
            ) {
                match self
                    .client
                    .get_file_diff(&repo.id, &branch.branch, &file.path)
                    .await
                {
                    Ok(response) => {
                        self.diff_file_lines.insert(file_idx, response.lines);
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Failed to load file diff: {}", e));
                        // Remove from expanded since load failed
                        self.diff_expanded.remove(&file_idx);
                    }
                }
            }
        }
        Ok(())
    }

    // ========== Unified diff navigation ==========

    /// Get total length of virtual diff list (files + expanded lines)
    pub fn diff_virtual_list_len(&self) -> usize {
        let mut len = 0;
        for (i, _) in self.diff_files.iter().enumerate() {
            len += 1; // File entry
            if self.diff_expanded.contains(&i) {
                len += self.diff_file_lines.get(&i).map(|l| l.len()).unwrap_or(0);
            }
        }
        len
    }

    /// Get current item at cursor position
    pub fn current_diff_item(&self) -> DiffItem {
        if self.diff_files.is_empty() {
            return DiffItem::None;
        }

        let mut pos = 0;
        for (file_idx, _) in self.diff_files.iter().enumerate() {
            // Check if cursor is on this file
            if pos == self.diff_cursor {
                return DiffItem::File(file_idx);
            }
            pos += 1;

            // Check if cursor is on one of this file's lines
            if self.diff_expanded.contains(&file_idx) {
                if let Some(lines) = self.diff_file_lines.get(&file_idx) {
                    for line_idx in 0..lines.len() {
                        if pos == self.diff_cursor {
                            return DiffItem::Line(file_idx, line_idx);
                        }
                        pos += 1;
                    }
                }
            }
        }

        DiffItem::None
    }

    /// Get file index from cursor position (even if on a line)
    #[allow(dead_code)]
    pub fn current_diff_file_idx(&self) -> Option<usize> {
        match self.current_diff_item() {
            DiffItem::File(idx) => Some(idx),
            DiffItem::Line(file_idx, _) => Some(file_idx),
            DiffItem::None => None,
        }
    }

    /// Move cursor up in diff view
    pub fn diff_move_up(&mut self) {
        if self.diff_cursor > 0 {
            self.diff_cursor -= 1;
            self.dirty.sidebar = true;
        }
    }

    /// Move cursor down in diff view
    pub fn diff_move_down(&mut self) {
        let max = self.diff_virtual_list_len();
        if max > 0 && self.diff_cursor < max - 1 {
            self.diff_cursor += 1;
            self.dirty.sidebar = true;
        }
    }

    /// Jump to previous file
    pub fn diff_prev_file(&mut self) {
        let mut pos = 0;
        let mut last_file_pos = 0;
        for (file_idx, _) in self.diff_files.iter().enumerate() {
            if pos >= self.diff_cursor {
                // Found current or past cursor, go to last file
                break;
            }
            last_file_pos = pos;
            pos += 1;
            if self.diff_expanded.contains(&file_idx) {
                pos += self
                    .diff_file_lines
                    .get(&file_idx)
                    .map(|l| l.len())
                    .unwrap_or(0);
            }
        }
        if self.diff_cursor > 0 {
            self.diff_cursor = last_file_pos;
            self.dirty.sidebar = true;
        }
    }

    /// Jump to next file
    pub fn diff_next_file(&mut self) {
        let mut pos = 0;
        for (file_idx, _) in self.diff_files.iter().enumerate() {
            if pos > self.diff_cursor {
                // Found next file after cursor
                self.diff_cursor = pos;
                self.dirty.sidebar = true;
                return;
            }
            pos += 1;
            if self.diff_expanded.contains(&file_idx) {
                pos += self
                    .diff_file_lines
                    .get(&file_idx)
                    .map(|l| l.len())
                    .unwrap_or(0);
            }
        }
    }

    /// Toggle expansion of current file (only works when cursor is on a file)
    pub fn toggle_diff_expand(&mut self) -> Option<AsyncAction> {
        if let DiffItem::File(file_idx) = self.current_diff_item() {
            if self.diff_expanded.contains(&file_idx) {
                // Collapse
                self.diff_expanded.remove(&file_idx);
                self.diff_file_lines.remove(&file_idx);
                None
            } else {
                // Expand - need to load diff content
                self.diff_expanded.insert(file_idx);
                Some(AsyncAction::LoadFileDiff)
            }
        } else {
            None
        }
    }

    /// Toggle diff fullscreen mode
    pub fn toggle_diff_fullscreen(&mut self) {
        self.diff_fullscreen = !self.diff_fullscreen;
    }

    // ========== Comment Operations ==========

    /// Start adding a line comment (only works when cursor is on a diff line)
    pub fn start_add_line_comment(&mut self) {
        if let DiffItem::Line(file_idx, line_idx) = self.current_diff_item() {
            if let (Some(file), Some(lines)) = (
                self.diff_files.get(file_idx),
                self.diff_file_lines.get(&file_idx),
            ) {
                if let Some(diff_line) = lines.get(line_idx) {
                    // Get actual line number from diff info
                    let line_number = diff_line
                        .new_lineno
                        .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));

                    self.input_mode = InputMode::AddLineComment {
                        file_path: file.path.clone(),
                        line_number,
                        line_type: diff_line.line_type,
                    };
                    self.input_buffer.clear();
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

        let comment_text = self.input_buffer.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();

        if comment_text.is_empty() {
            self.status_message = Some("Comment cannot be empty".to_string());
            return Ok(());
        }

        // Get current repo and branch
        if let (Some(repo), Some(branch)) = (
            self.repos.get(self.repo_idx).cloned(),
            self.worktrees.get(self.branch_idx).cloned(),
        ) {
            match self
                .client
                .create_line_comment(
                    &repo.id,
                    &branch.branch,
                    &file_path,
                    line_number,
                    line_type,
                    &comment_text,
                )
                .await
            {
                Ok(comment) => {
                    self.line_comments.push(comment);
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
        if let (Some(repo), Some(branch)) = (
            self.repos.get(self.repo_idx).cloned(),
            self.worktrees.get(self.branch_idx).cloned(),
        ) {
            match self
                .client
                .list_line_comments(&repo.id, &branch.branch, None)
                .await
            {
                Ok(comments) => {
                    self.line_comments = comments;
                }
                Err(e) => {
                    tracing::warn!("Failed to load comments: {}", e);
                    self.line_comments.clear();
                }
            }
        }
        Ok(())
    }

    /// Get comments for a specific file and line
    pub fn get_line_comment(&self, file_path: &str, line_number: i32) -> Option<&LineCommentInfo> {
        self.line_comments
            .iter()
            .find(|c| c.file_path == file_path && c.line_number == line_number)
    }

    /// Check if a line has a comment
    pub fn has_line_comment(&self, file_path: &str, line_number: i32) -> bool {
        self.get_line_comment(file_path, line_number).is_some()
    }

    /// Count comments for a specific file
    pub fn count_file_comments(&self, file_path: &str) -> usize {
        self.line_comments
            .iter()
            .filter(|c| c.file_path == file_path)
            .count()
    }

    /// Start editing an existing comment on current line
    pub fn start_edit_line_comment(&mut self) {
        // Extract needed data first to avoid borrow conflicts
        let edit_info: Option<(String, String, i32, String)> = {
            if let DiffItem::Line(file_idx, line_idx) = self.current_diff_item() {
                if let (Some(file), Some(lines)) = (
                    self.diff_files.get(file_idx),
                    self.diff_file_lines.get(&file_idx),
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
            self.input_mode = InputMode::EditLineComment {
                comment_id,
                file_path,
                line_number,
            };
            self.input_buffer = comment_text;
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

        let comment_text = self.input_buffer.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();

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
                if let Some(comment) = self.line_comments.iter_mut().find(|c| c.id == comment_id) {
                    comment.comment = updated.comment;
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
            if let (Some(file), Some(lines)) = (
                self.diff_files.get(file_idx),
                self.diff_file_lines.get(&file_idx),
            ) {
                if let Some(diff_line) = lines.get(line_idx) {
                    let line_number = diff_line
                        .new_lineno
                        .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));

                    if let Some(comment) = self
                        .line_comments
                        .iter()
                        .find(|c| c.file_path == file.path && c.line_number == line_number)
                    {
                        let comment_id = comment.id.clone();

                        match self.client.delete_line_comment(&comment_id).await {
                            Ok(_) => {
                                self.line_comments.retain(|c| c.id != comment_id);
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
        let mut all_lines: Vec<(usize, usize, i32, String)> = Vec::new();
        for (file_idx, file) in self.diff_files.iter().enumerate() {
            if self.diff_expanded.contains(&file_idx) {
                if let Some(lines) = self.diff_file_lines.get(&file_idx) {
                    for (line_idx, diff_line) in lines.iter().enumerate() {
                        let line_number = diff_line
                            .new_lineno
                            .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));
                        all_lines.push((file_idx, line_idx, line_number, file.path.clone()));
                    }
                }
            }
        }

        // Find current position in flat list
        let current_pos = all_lines
            .iter()
            .position(|(f, l, _, _)| *f == current_file_idx && *l >= current_line_idx)
            .unwrap_or(0);

        // Find next comment after current position
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().skip(current_pos + 1) {
            if self.has_line_comment(file_path, *line_number) {
                self.diff_cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        // Wrap around - search from beginning
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().take(current_pos + 1) {
            if self.has_line_comment(file_path, *line_number) {
                self.diff_cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
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
        let mut all_lines: Vec<(usize, usize, i32, String)> = Vec::new();
        for (file_idx, file) in self.diff_files.iter().enumerate() {
            if self.diff_expanded.contains(&file_idx) {
                if let Some(lines) = self.diff_file_lines.get(&file_idx) {
                    for (line_idx, diff_line) in lines.iter().enumerate() {
                        let line_number = diff_line
                            .new_lineno
                            .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));
                        all_lines.push((file_idx, line_idx, line_number, file.path.clone()));
                    }
                }
            }
        }

        // Find current position in flat list
        let current_pos = all_lines
            .iter()
            .position(|(f, l, _, _)| *f == current_file_idx && *l >= current_line_idx)
            .unwrap_or(all_lines.len());

        // Find previous comment before current position
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().take(current_pos).rev()
        {
            if self.has_line_comment(file_path, *line_number) {
                self.diff_cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        // Wrap around - search from end
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().skip(current_pos).rev()
        {
            if self.has_line_comment(file_path, *line_number) {
                self.diff_cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        self.status_message = Some("No comments to jump to".to_string());
    }

    /// Calculate cursor position for a specific file and line
    fn calculate_cursor_for_line(&self, target_file_idx: usize, target_line_idx: usize) -> usize {
        let mut cursor = 0;
        for (file_idx, _) in self.diff_files.iter().enumerate() {
            if file_idx == target_file_idx {
                // Found the file, add the line offset
                return cursor + 1 + target_line_idx; // +1 for file header
            }
            cursor += 1; // File header
            if self.diff_expanded.contains(&file_idx) {
                if let Some(lines) = self.diff_file_lines.get(&file_idx) {
                    cursor += lines.len();
                }
            }
        }
        cursor
    }

    /// Submit all comments as a review to Claude
    pub async fn submit_review_to_claude(&mut self) -> Result<()> {
        if self.line_comments.is_empty() {
            self.status_message = Some("No comments to submit".to_string());
            return Ok(());
        }

        // Build the review prompt
        let mut prompt = String::from("Please help me review the following code changes:\n\n");

        // Group comments by file
        let mut by_file: std::collections::HashMap<String, Vec<&LineCommentInfo>> =
            std::collections::HashMap::new();
        for comment in &self.line_comments {
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
        if self.terminal_stream.is_none() && self.active_session_id.is_some() {
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

/// Convert vt100 color to ratatui color
fn vt100_color_to_ratatui(color: vt100::Color) -> ratatui::style::Color {
    match color {
        vt100::Color::Default => ratatui::style::Color::Reset,
        vt100::Color::Idx(idx) => ratatui::style::Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
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

    // Render interval (~30fps for smoother rendering, reduces flicker)
    let mut render_interval = tokio::time::interval(std::time::Duration::from_millis(33));
    render_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Minimum time between renders to prevent flicker during high-frequency updates
    // 100ms (10fps) provides stable rendering during plan mode while still feeling responsive
    let min_render_interval = std::time::Duration::from_millis(100);
    let mut last_render = std::time::Instant::now();

    // Fallback timers
    let mut last_refresh = std::time::Instant::now();
    let mut last_resubscribe_attempt = std::time::Instant::now();
    let refresh_interval = std::time::Duration::from_secs(5);
    let resubscribe_interval = std::time::Duration::from_secs(10);

    // State
    let mut dirty = true;
    let mut force_render = false; // Force immediate render (for user input)
    let mut pty_received = false; // Track if PTY data was received (needs hash check)
    let mut pending_action: Option<AsyncAction> = None;

    // Main loop with tokio::select!
    loop {
        tokio::select! {
            biased; // Check branches in priority order

            // 1. Highest priority: keyboard input (immediate response)
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
                        dirty = true;
                        force_render = true; // User input needs immediate feedback
                    }
                    Event::Resize(cols, rows) => {
                        let _ = app.resize_terminal(rows, cols).await;
                        // Invalidate cache when resize happens
                        app.cached_terminal_lines.clear();
                        app.cached_terminal_size = (0, 0);
                        dirty = true;
                        // Don't force render on resize - let it be throttled to reduce flicker
                    }
                    Event::Mouse(mouse) => {
                        handle_mouse_sync(&mut app, mouse);
                        dirty = true;
                        force_render = true;
                    }
                    _ => {}
                }
            }

            // 2. Terminal PTY output
            Some(data) = async {
                match app.terminal_stream.as_mut() {
                    Some(stream) => stream.output_rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Ok(mut parser) = app.terminal_parser.lock() {
                    parser.process(&data);
                }
                // Don't set dirty immediately - mark for hash check on render tick
                // This prevents excessive redraws when content hasn't visually changed
                pty_received = true;
            }

            // 3. Daemon events
            Some(event) = async {
                match app.event_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                // Only mark dirty if the event actually changed something visible
                if app.handle_daemon_event(event) {
                    dirty = true;
                }
            }

            // 4. Render tick + execute pending async actions
            _ = render_interval.tick() => {
                // Execute pending async action
                if let Some(action) = pending_action.take() {
                    if let Err(e) = app.execute_async_action(action).await {
                        app.error_message = Some(format!("{}", e));
                    }
                    dirty = true;
                    // Also check app.dirty to catch updates from async action
                    if app.dirty.any() {
                        dirty = true;
                        app.dirty.clear();
                    }
                }

                // Check if terminal content actually changed (only if PTY data was received)
                if pty_received {
                    if app.update_terminal_hash() {
                        dirty = true;
                    }
                    pty_received = false;
                }

                // Check if we need to resubscribe (event channel disconnected)
                if app.needs_resubscribe() {
                    // Fallback: Periodic session refresh while disconnected
                    if last_refresh.elapsed() >= refresh_interval {
                        let _ = app.refresh_sessions().await;
                        last_refresh = std::time::Instant::now();
                        dirty = true;
                    }

                    // Periodically attempt to resubscribe
                    if last_resubscribe_attempt.elapsed() >= resubscribe_interval {
                        app.try_resubscribe().await;
                        last_resubscribe_attempt = std::time::Instant::now();
                    }
                }

                // Render if:
                // 1. force_render is set (user input - needs immediate feedback), OR
                // 2. dirty and minimum interval has passed (background updates - throttled)
                if dirty && (force_render || last_render.elapsed() >= min_render_interval) {
                    // Use synchronized update to prevent flicker
                    // Terminal will buffer all output until EndSynchronizedUpdate
                    execute!(terminal.backend_mut(), BeginSynchronizedUpdate)
                        .map_err(TuiError::Render)?;
                    terminal.draw(|f| draw(f, &app)).map_err(TuiError::Render)?;
                    execute!(terminal.backend_mut(), EndSynchronizedUpdate)
                        .map_err(TuiError::Render)?;
                    dirty = false;
                    force_render = false;
                    // Clear app dirty flags after render
                    app.dirty.clear();
                    last_render = std::time::Instant::now();
                }
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
