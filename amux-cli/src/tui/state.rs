//! TUI state types and enums
//!
//! This module contains all the state type definitions used by the TUI application.
//! Separating types from implementation improves maintainability and enables
//! independent testing of state logic.

use std::collections::HashSet;

/// Focus position in the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Sidebar,   // Tree view: worktrees with nested sessions
    GitStatus, // Git status panel
    Terminal,  // Terminal interaction area
    DiffFiles, // Diff file list (with inline expansion)
}

/// Sidebar item in tree view
#[derive(Debug, Clone, PartialEq)]
pub enum SidebarItem {
    Worktree(usize),       // Worktree at index
    Session(usize, usize), // (worktree_idx, session_idx within worktree)
    None,
}

/// Git status section
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitSection {
    Staged,
    Unstaged,
    Untracked,
}

/// A file with its git status (client-side representation)
#[derive(Debug, Clone)]
pub struct GitStatusFile {
    pub path: String,
    pub status: i32, // FileStatus enum value
    pub section: GitSection,
}

/// Item in the git status panel
#[derive(Debug, Clone, PartialEq)]
pub enum GitPanelItem {
    Section(GitSection), // Section header
    File(usize),         // File at index in git_status_files
    None,
}

/// Right panel view mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum RightPanelView {
    #[default]
    Terminal,
    Diff,
}

/// Current item in diff view (for unified navigation)
#[derive(Debug, Clone, PartialEq)]
pub enum DiffItem {
    File(usize),        // File at index
    Line(usize, usize), // (file_idx, line_idx within that file)
    None,               // Empty state
}

/// Delete target for confirmation
#[derive(Debug, Clone, PartialEq)]
pub enum DeleteTarget {
    Worktree { repo_id: String, branch: String },
    Session { session_id: String, name: String },
}

/// Exit cleanup action for sessions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExitCleanupAction {
    Destroy, // Destroy all sessions (delete data)
    Stop,    // Stop sessions (kill PTY, keep metadata)
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
    AddLineComment {
        // Adding a comment to a diff line
        file_path: String,
        line_number: i32,
        line_type: i32,
    },
    EditLineComment {
        // Editing an existing comment
        comment_id: String,
        file_path: String,
        line_number: i32,
    },
    // TODO modes
    TodoPopup,
    AddTodo {
        parent_id: Option<String>,
    },
    EditTodo {
        todo_id: String,
    },
    EditTodoDescription {
        todo_id: String,
    },
    ConfirmDeleteTodo {
        todo_id: String,
        title: String,
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
pub struct DirtyFlags {
    pub sidebar: bool, // repo/branch/session list changed
}

impl DirtyFlags {
    #[allow(dead_code)]
    pub fn any(&self) -> bool {
        self.sidebar
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod dirty_flags_tests {
    use super::*;

    #[test]
    fn test_dirty_flags_default() {
        let flags = DirtyFlags::default();
        assert!(!flags.sidebar);
        assert!(!flags.any());
    }

    #[test]
    fn test_dirty_flags_any_sidebar() {
        let flags = DirtyFlags { sidebar: true };
        assert!(flags.any());
    }

    #[test]
    fn test_dirty_flags_any_none() {
        let flags = DirtyFlags { sidebar: false };
        assert!(!flags.any());
    }

    #[test]
    fn test_dirty_flags_clear() {
        let mut flags = DirtyFlags { sidebar: true };
        assert!(flags.any());

        flags.clear();
        assert!(!flags.sidebar);
        assert!(!flags.any());
    }

    #[test]
    fn test_dirty_flags_clone() {
        let flags1 = DirtyFlags { sidebar: true };
        let flags2 = flags1.clone();

        assert_eq!(flags1.sidebar, flags2.sidebar);
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
    ConfirmDelete {
        target: DeleteTarget,
        action: ExitCleanupAction,
    },
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
    // Comment actions
    LoadComments,
    SubmitLineComment,
    UpdateLineComment,
    DeleteLineComment,
    SubmitReviewToClaude,
    // Tree view actions
    LoadWorktreeSessions {
        wt_idx: usize,
    },
    // Git status actions
    LoadGitStatus,
    StageFile {
        file_path: String,
    },
    UnstageFile {
        file_path: String,
    },
    StageAll,
    UnstageAll,
    GitPush,
    GitPull,
    // Shell session action
    SwitchToShell,
    // TODO actions
    LoadTodos,
    CreateTodo {
        title: String,
        description: Option<String>,
        parent_id: Option<String>,
    },
    ToggleTodo {
        todo_id: String,
    },
    DeleteTodo {
        todo_id: String,
    },
    UpdateTodo {
        todo_id: String,
        title: Option<String>,
        description: Option<String>,
    },
    ReorderTodo {
        todo_id: String,
        new_order: i32,
        new_parent_id: Option<String>,
    },
}

/// Default expanded git sections
pub fn default_expanded_git_sections() -> HashSet<GitSection> {
    let mut set = HashSet::new();
    set.insert(GitSection::Staged);
    set.insert(GitSection::Unstaged);
    set.insert(GitSection::Untracked);
    set
}

// ============ Grouped State Types ============
// These types group related fields from App for better organization.
// They are designed to be used as embedded structs within App.

use amux_proto::daemon::{
    DiffFileInfo, DiffLine, LineCommentInfo, RepoInfo, SessionInfo, TodoItem, WorktreeInfo,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Per-repository state containing all repo-specific data and UI state.
/// This structure encapsulates all state that should be preserved when switching
/// between repositories, eliminating the need for save/restore logic.
pub struct RepoState {
    /// Basic repo info (id, path, name) from daemon
    pub info: RepoInfo,

    // ============ Data ============
    /// Worktrees with paths (branches that have been checked out)
    pub worktrees: Vec<WorktreeInfo>,
    /// Branches without worktrees (available for checkout)
    pub available_branches: Vec<WorktreeInfo>,
    /// Sessions for the current worktree
    pub sessions: Vec<SessionInfo>,

    // ============ Selection Indices ============
    /// Currently selected worktree/branch index
    pub branch_idx: usize,
    /// Currently selected session index within branch
    pub session_idx: usize,
    /// Selection index in add worktree popup
    pub add_worktree_idx: usize,

    // ============ Sidebar State (per-repo) ============
    /// Cursor position in sidebar virtual list
    pub sidebar_cursor: usize,
    /// Which worktrees are expanded (by index)
    pub expanded_worktrees: HashSet<usize>,
    /// Sessions grouped by worktree index (cache for tree view)
    pub sessions_by_worktree: HashMap<usize, Vec<SessionInfo>>,

    // ============ View State ============
    /// Git status panel state
    pub git: GitState,
    /// Diff view state
    pub diff: DiffState,
    /// Line comments for current branch
    pub line_comments: Vec<LineCommentInfo>,
}

impl RepoState {
    /// Create a new RepoState from repo info
    pub fn new(info: RepoInfo) -> Self {
        Self {
            info,
            worktrees: Vec::new(),
            available_branches: Vec::new(),
            sessions: Vec::new(),
            branch_idx: 0,
            session_idx: 0,
            add_worktree_idx: 0,
            sidebar_cursor: 0,
            expanded_worktrees: HashSet::new(),
            sessions_by_worktree: HashMap::new(),
            git: GitState::default(),
            diff: DiffState::default(),
            line_comments: Vec::new(),
        }
    }

    /// Get the currently selected worktree
    pub fn current_worktree(&self) -> Option<&WorktreeInfo> {
        self.worktrees.get(self.branch_idx)
    }

    /// Get the currently selected session
    pub fn current_session(&self) -> Option<&SessionInfo> {
        self.sessions.get(self.session_idx)
    }

    /// Calculate total sidebar items count (worktrees + expanded sessions)
    pub fn calculate_sidebar_total(&self) -> usize {
        let mut count = self.worktrees.len();
        for &wt_idx in &self.expanded_worktrees {
            if let Some(sessions) = self.sessions_by_worktree.get(&wt_idx) {
                count += sessions.len();
            }
        }
        count.max(1)
    }

    /// Clamp all indices to valid ranges
    pub fn clamp_indices(&mut self) {
        if self.worktrees.is_empty() {
            self.branch_idx = 0;
            self.session_idx = 0;
            self.sidebar_cursor = 0;
        } else {
            if self.branch_idx >= self.worktrees.len() {
                self.branch_idx = self.worktrees.len() - 1;
            }
            if self.sessions.is_empty() {
                self.session_idx = 0;
            } else if self.session_idx >= self.sessions.len() {
                self.session_idx = self.sessions.len() - 1;
            }
            let max_cursor = self.calculate_sidebar_total().saturating_sub(1);
            if self.sidebar_cursor > max_cursor {
                self.sidebar_cursor = max_cursor;
            }
        }
    }
}

/// Terminal-related state
pub struct TerminalState {
    /// VT100 parser for current session
    pub parser: Arc<Mutex<vt100::Parser>>,
    /// Per-session parsers (cached)
    pub session_parsers: HashMap<String, Arc<Mutex<vt100::Parser>>>,
    /// Currently active session ID
    pub active_session_id: Option<String>,
    /// Session ID before switching to shell (for Ctrl+` toggle)
    pub session_before_shell: Option<String>,
    /// Whether in interactive mode
    pub is_interactive: bool,
    /// Terminal mode (Normal/Insert)
    pub mode: TerminalMode,
    /// Scroll offset for terminal content
    pub scroll_offset: usize,
    /// Whether terminal is fullscreen
    pub fullscreen: bool,
    /// Terminal columns
    pub cols: Option<u16>,
    /// Terminal rows
    pub rows: Option<u16>,
}

impl Default for TerminalState {
    fn default() -> Self {
        Self {
            parser: Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))),
            session_parsers: HashMap::new(),
            active_session_id: None,
            session_before_shell: None,
            is_interactive: false,
            mode: TerminalMode::Normal,
            scroll_offset: 0,
            fullscreen: false,
            cols: None,
            rows: None,
        }
    }
}

/// Diff view state
#[derive(Default)]
pub struct DiffState {
    /// List of diff files
    pub files: Vec<DiffFileInfo>,
    /// Which files are expanded (by index)
    pub expanded: HashSet<usize>,
    /// Lines per expanded file
    pub file_lines: HashMap<usize, Vec<DiffLine>>,
    /// Unified cursor position in virtual list
    pub cursor: usize,
    /// Scroll offset for rendering
    pub scroll_offset: usize,
    /// Whether diff is fullscreen
    pub fullscreen: bool,
}

/// Git status panel state
pub struct GitState {
    /// All files (staged + unstaged + untracked)
    pub files: Vec<GitStatusFile>,
    /// Cursor in virtual list
    pub cursor: usize,
    /// Expanded sections
    pub expanded_sections: HashSet<GitSection>,
    /// File to auto-expand in diff view
    pub pending_diff_file: Option<String>,
}

impl Default for GitState {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            cursor: 0,
            expanded_sections: default_expanded_git_sections(),
            pending_diff_file: None,
        }
    }
}

/// Sidebar state (global UI state only, per-repo state is in RepoState)
pub struct SidebarState {
    /// Whether git panel is enabled
    pub git_panel_enabled: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            git_panel_enabled: true,
        }
    }
}

/// TODO state
#[derive(Default)]
pub struct TodoState {
    /// All TODO items
    pub items: Vec<TodoItem>,
    /// Cursor position
    pub cursor: usize,
    /// Expanded TODO items (by ID)
    #[allow(dead_code)]
    pub expanded: HashSet<String>,
    /// Scroll offset
    pub scroll_offset: usize,
    /// Whether to show completed items
    pub show_completed: bool,
    /// Display order (indices in tree order)
    pub display_order: Vec<usize>,
}

impl TodoState {
    pub fn new() -> Self {
        Self {
            show_completed: true,
            ..Default::default()
        }
    }
}

// VirtualList implementations for state types
use super::widgets::VirtualList;

impl VirtualList for RepoState {
    fn virtual_len(&self) -> usize {
        self.calculate_sidebar_total()
    }

    fn cursor(&self) -> usize {
        self.sidebar_cursor
    }

    fn set_cursor(&mut self, pos: usize) {
        self.sidebar_cursor = pos;
    }
}

impl VirtualList for DiffState {
    fn virtual_len(&self) -> usize {
        let mut count = 0;
        for (idx, _) in self.files.iter().enumerate() {
            count += 1; // File header
            if self.expanded.contains(&idx) {
                if let Some(lines) = self.file_lines.get(&idx) {
                    count += lines.len();
                }
            }
        }
        count.max(1)
    }

    fn cursor(&self) -> usize {
        self.cursor
    }

    fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos;
    }
}

impl VirtualList for GitState {
    fn virtual_len(&self) -> usize {
        let mut count = 0;
        for section in [
            GitSection::Staged,
            GitSection::Unstaged,
            GitSection::Untracked,
        ] {
            if self.expanded_sections.contains(&section) {
                count += 1; // Section header
                            // Count files in this section
                count += self.files.iter().filter(|f| f.section == section).count();
            }
        }
        count.max(1)
    }

    fn cursor(&self) -> usize {
        self.cursor
    }

    fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos;
    }
}

impl VirtualList for TodoState {
    fn virtual_len(&self) -> usize {
        self.display_order.len().max(1)
    }

    fn cursor(&self) -> usize {
        self.cursor
    }

    fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos;
    }
}
