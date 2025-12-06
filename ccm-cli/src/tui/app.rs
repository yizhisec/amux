//! TUI application state machine

use crate::client::Client;
use crate::error::TuiError;
use ccm_proto::daemon::{
    event as daemon_event, AttachInput, DiffFileInfo, DiffLine, Event as DaemonEvent, RepoInfo,
    SessionInfo, WorktreeInfo,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{debug, warn};

type Result<T> = std::result::Result<T, TuiError>;

use super::input::handle_input_sync;
use super::ui::draw;

/// Focus position in the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Branches,  // Branch list in sidebar
    Sessions,  // Session list in sidebar
    Terminal,  // Terminal interaction area
    DiffFiles, // Diff file list (with inline expansion)
}

/// Right panel view mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum RightPanelView {
    #[default]
    Terminal,
    Diff,
}

/// Delete target for confirmation
#[derive(Debug, Clone, PartialEq)]
pub enum DeleteTarget {
    Worktree { repo_id: String, branch: String },
    Session { session_id: String, name: String },
}

/// Input mode for text entry
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    NewBranch, // Entering new branch name (deprecated, use AddWorktree)
    AddWorktree {
        base_branch: Option<String>, // Branch to create from (None = HEAD)
    }, // Adding worktree (select branch or enter new name)
    RenameSession {
        session_id: String,
    }, // Renaming a session
    ConfirmDelete(DeleteTarget), // Confirm deletion
    ConfirmDeleteBranch(String), // Confirm deleting branch after worktree (branch name)
    ConfirmDeleteWorktreeSessions {
        // Worktree has sessions, confirm deleting them first
        repo_id: String,
        branch: String,
        session_count: i32,
    },
}

/// Terminal mode (vim-style)
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalMode {
    Normal, // View/scroll mode
    Insert, // Interactive input mode
}

/// Prefix key mode state
#[derive(Debug, Clone, PartialEq)]
pub enum PrefixMode {
    /// Normal mode - no prefix active
    None,
    /// Waiting for command after Ctrl+s prefix
    WaitingForCommand,
}

/// Tracks which UI components need redrawing
#[derive(Default, Clone)]
#[allow(dead_code)]
pub struct DirtyFlags {
    pub sidebar: bool,     // repo/branch/session list changed
    pub terminal: bool,    // terminal content changed
    pub status_bar: bool,  // status bar info changed
    pub full_redraw: bool, // need full redraw (e.g., resize)
}

#[allow(dead_code)]
impl DirtyFlags {
    pub fn any(&self) -> bool {
        self.sidebar || self.terminal || self.status_bar || self.full_redraw
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn mark_all(&mut self) {
        self.sidebar = true;
        self.terminal = true;
        self.status_bar = true;
    }
}

/// Async actions that can be queued from sync input handlers
#[derive(Debug)]
#[allow(dead_code)]
pub enum AsyncAction {
    RefreshAll,
    RefreshSessions,
    RefreshBranches,
    CreateSession,
    SubmitInput,
    SubmitRenameSession,
    SubmitAddWorktree,
    ConfirmDelete,
    ConfirmDeleteBranch,
    ConfirmDeleteWorktreeSessions,
    DestroySession {
        session_id: String,
    },
    RenameSession {
        session_id: String,
        new_name: String,
    },
    ConnectStream,
    ResizeTerminal {
        rows: u16,
        cols: u16,
    },
    SendToTerminal {
        data: Vec<u8>,
    },
    // Diff actions
    SwitchToDiffView,
    LoadDiffFiles,
    LoadFileDiff,
}

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

    // Diff state
    pub right_panel_view: RightPanelView,
    pub diff_files: Vec<DiffFileInfo>,
    pub diff_file_idx: usize,
    pub diff_expanded_idx: Option<usize>, // Which file is expanded (None = all collapsed)
    pub diff_lines: Vec<DiffLine>,        // Lines for expanded file
    pub diff_scroll_offset: usize,        // Scroll offset within expanded content
    pub diff_fullscreen: bool,

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
}

impl App {
    pub async fn new(client: Client) -> Result<Self> {
        let mut app = Self {
            client,
            repo_idx: 0,
            branch_idx: 0,
            session_idx: 0,
            focus: Focus::Branches,
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
            right_panel_view: RightPanelView::Terminal,
            diff_files: Vec::new(),
            diff_file_idx: 0,
            diff_expanded_idx: None,
            diff_lines: Vec::new(),
            diff_scroll_offset: 0,
            diff_fullscreen: false,
            should_quit: false,
            error_message: None,
            status_message: None,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            event_rx: None,
            prefix_mode: PrefixMode::None,
            dirty: DirtyFlags::default(),
        };

        // Load initial data
        app.refresh_all().await?;

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
            Focus::Branches => self.branch_idx,
            Focus::Sessions => self.session_idx,
            Focus::Terminal => 0,
            Focus::DiffFiles => self.diff_file_idx,
        }
    }

    // ========== Sync versions for responsive input handling ==========

    /// Move selection up (sync version - returns async action if needed)
    pub fn select_prev_sync(&mut self) -> Option<AsyncAction> {
        match self.focus {
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
                self.diff_select_prev();
            }
        }
        None
    }

    /// Move selection down (sync version - returns async action if needed)
    pub fn select_next_sync(&mut self) -> Option<AsyncAction> {
        match self.focus {
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
                self.diff_select_next();
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
    pub fn exit_to_normal_mode(&mut self) {
        self.terminal_mode = TerminalMode::Normal;
        deactivate_ime();
    }

    /// Exit terminal mode (back to Sessions)
    pub fn exit_terminal(&mut self) {
        if self.terminal_fullscreen {
            self.terminal_fullscreen = false;
        } else {
            self.focus = Focus::Sessions;
            self.terminal_mode = TerminalMode::Normal;
            self.is_interactive = false;
        }
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
        self.focus = Focus::Sessions;
    }

    /// Create new session and enter interactive mode
    pub async fn create_new(&mut self) -> Result<()> {
        match self.focus {
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
                        .create_session(&repo.id, &branch.branch, None)
                        .await
                    {
                        Ok(session) => {
                            self.refresh_sessions().await?;
                            // Find and select the new session
                            if let Some(idx) = self.sessions.iter().position(|s| s.id == session.id)
                            {
                                self.session_idx = idx;
                                self.update_active_session().await;
                            }
                            self.enter_terminal().await?;
                        }
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                        }
                    }
                }
            }
            Focus::Terminal | Focus::DiffFiles => {}
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
                .create_session(&repo.id, &branch_name, None)
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
                        }
                    }
                    self.focus = Focus::Sessions;
                    self.enter_terminal().await?;
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
                // Update local session list
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
                    session.name = new_name;
                }
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
            Focus::Terminal | Focus::DiffFiles => {}
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

    /// Handle a daemon event
    fn handle_daemon_event(&mut self, event: DaemonEvent) {
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
                        }
                    }
                }
            }
            Some(daemon_event::Event::SessionDestroyed(e)) => {
                debug!("Event: SessionDestroyed {}", e.session_id);
                // Remove session from list
                self.sessions.retain(|s| s.id != e.session_id);
                // Clamp session index
                if !self.sessions.is_empty() && self.session_idx >= self.sessions.len() {
                    self.session_idx = self.sessions.len() - 1;
                }
            }
            Some(daemon_event::Event::SessionNameUpdated(e)) => {
                debug!(
                    "Event: SessionNameUpdated {} -> {}",
                    e.session_id, e.new_name
                );
                // Update session name in list
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == e.session_id) {
                    session.name = e.new_name;
                }
            }
            Some(daemon_event::Event::SessionStatusChanged(e)) => {
                debug!(
                    "Event: SessionStatusChanged {} -> {}",
                    e.session_id, e.new_status
                );
                // Update session status in list
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == e.session_id) {
                    session.status = e.new_status;
                }
            }
            None => {}
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
        }
        Ok(())
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

    /// Get terminal lines for rendering
    pub fn get_terminal_lines(&self, height: u16, width: u16) -> Vec<ratatui::text::Line<'static>> {
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

    // ========== Diff View ==========

    /// Switch to diff view
    pub async fn switch_to_diff_view(&mut self) -> Result<()> {
        self.right_panel_view = RightPanelView::Diff;
        self.focus = Focus::DiffFiles;
        self.load_diff_files().await?;
        Ok(())
    }

    /// Switch back to terminal view
    pub fn switch_to_terminal_view(&mut self) {
        self.right_panel_view = RightPanelView::Terminal;
        self.focus = Focus::Sessions;
        self.diff_files.clear();
        self.diff_lines.clear();
        self.diff_file_idx = 0;
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
                    self.diff_file_idx = 0;
                    self.diff_lines.clear();
                    self.diff_scroll_offset = 0;

                    // Auto-load first file if available
                    if !self.diff_files.is_empty() {
                        self.load_file_diff().await?;
                    }
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to load diff: {}", e));
                }
            }
        }
        Ok(())
    }

    /// Load diff content for selected file
    pub async fn load_file_diff(&mut self) -> Result<()> {
        if let (Some(repo), Some(branch), Some(file)) = (
            self.repos.get(self.repo_idx).cloned(),
            self.worktrees.get(self.branch_idx).cloned(),
            self.diff_files.get(self.diff_file_idx).cloned(),
        ) {
            match self
                .client
                .get_file_diff(&repo.id, &branch.branch, &file.path)
                .await
            {
                Ok(response) => {
                    self.diff_lines = response.lines;
                    self.diff_scroll_offset = 0;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to load file diff: {}", e));
                }
            }
        }
        Ok(())
    }

    /// Select previous diff file
    pub fn diff_select_prev(&mut self) -> Option<AsyncAction> {
        if self.diff_file_idx > 0 {
            self.diff_file_idx -= 1;
            self.diff_scroll_offset = 0; // Reset scroll when changing files
            self.dirty.sidebar = true;
        }
        None
    }

    /// Select next diff file
    pub fn diff_select_next(&mut self) -> Option<AsyncAction> {
        if !self.diff_files.is_empty() && self.diff_file_idx < self.diff_files.len() - 1 {
            self.diff_file_idx += 1;
            self.diff_scroll_offset = 0; // Reset scroll when changing files
            self.dirty.sidebar = true;
        }
        None
    }

    /// Toggle expansion of current diff file
    pub fn toggle_diff_expand(&mut self) -> Option<AsyncAction> {
        if self.diff_files.is_empty() {
            return None;
        }

        if self.diff_expanded_idx == Some(self.diff_file_idx) {
            // Collapse
            self.diff_expanded_idx = None;
            self.diff_lines.clear();
            self.diff_scroll_offset = 0;
            None
        } else {
            // Expand - need to load diff content
            self.diff_expanded_idx = Some(self.diff_file_idx);
            self.diff_scroll_offset = 0;
            Some(AsyncAction::LoadFileDiff)
        }
    }

    /// Toggle diff fullscreen mode
    pub fn toggle_diff_fullscreen(&mut self) {
        self.diff_fullscreen = !self.diff_fullscreen;
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

    // Render interval (~60fps)
    let mut render_interval = tokio::time::interval(std::time::Duration::from_millis(16));
    render_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Fallback timers
    let mut last_refresh = std::time::Instant::now();
    let mut last_resubscribe_attempt = std::time::Instant::now();
    let refresh_interval = std::time::Duration::from_secs(5);
    let resubscribe_interval = std::time::Duration::from_secs(10);

    // State
    let mut dirty = true;
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
                    }
                    Event::Resize(cols, rows) => {
                        let _ = app.resize_terminal(rows, cols).await;
                        dirty = true;
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
                dirty = true;
            }

            // 3. Daemon events
            Some(event) = async {
                match app.event_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                app.handle_daemon_event(event);
                dirty = true;
            }

            // 4. Render tick + execute pending async actions
            _ = render_interval.tick() => {
                // Execute pending async action
                if let Some(action) = pending_action.take() {
                    if let Err(e) = app.execute_async_action(action).await {
                        app.error_message = Some(format!("{}", e));
                    }
                    dirty = true;
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

                // Only render if dirty
                if dirty {
                    terminal.draw(|f| draw(f, &app)).map_err(TuiError::Render)?;
                    dirty = false;
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
