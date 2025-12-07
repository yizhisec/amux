//! TUI state types and enums
//!
//! This module contains all the state type definitions used by the TUI application.
//! Separating types from implementation improves maintainability and enables
//! independent testing of state logic.

use std::collections::HashSet;

/// Focus position in the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Branches,  // Branch list in sidebar (legacy, used when tree view disabled)
    Sessions,  // Session list in sidebar (legacy, used when tree view disabled)
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
    pub sidebar: bool,  // repo/branch/session list changed
    pub terminal: bool, // terminal content changed
}

impl DirtyFlags {
    pub fn any(&self) -> bool {
        self.sidebar || self.terminal
    }

    pub fn clear(&mut self) {
        *self = Self::default();
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

use ccm_proto::daemon::{DiffFileInfo, DiffLine, SessionInfo, TodoItem};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Terminal-related state
pub struct TerminalState {
    /// VT100 parser for current session
    pub parser: Arc<Mutex<vt100::Parser>>,
    /// Per-session parsers (cached)
    pub session_parsers: HashMap<String, Arc<Mutex<vt100::Parser>>>,
    /// Currently active session ID
    pub active_session_id: Option<String>,
    /// Whether in interactive mode
    pub is_interactive: bool,
    /// Terminal mode (Normal/Insert)
    pub mode: TerminalMode,
    /// Scroll offset for terminal content
    pub scroll_offset: usize,
    /// Whether terminal is fullscreen
    pub fullscreen: bool,
    /// Hash of terminal content for change detection
    pub last_hash: u64,
    /// Cached terminal lines for rendering
    pub cached_lines: Vec<ratatui::text::Line<'static>>,
    /// Cached terminal size (height, width)
    pub cached_size: (u16, u16),
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
            is_interactive: false,
            mode: TerminalMode::Normal,
            scroll_offset: 0,
            fullscreen: false,
            last_hash: 0,
            cached_lines: Vec::new(),
            cached_size: (0, 0),
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

/// Sidebar state (tree view with worktrees and sessions)
pub struct SidebarState {
    /// Whether tree view is enabled
    pub tree_view_enabled: bool,
    /// Which worktrees are expanded (by index)
    pub expanded_worktrees: HashSet<usize>,
    /// Cursor position in virtual list
    pub cursor: usize,
    /// Sessions grouped by worktree index
    pub sessions_by_worktree: HashMap<usize, Vec<SessionInfo>>,
    /// Whether git panel is enabled
    pub git_panel_enabled: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            tree_view_enabled: true,
            expanded_worktrees: HashSet::new(),
            cursor: 0,
            sessions_by_worktree: HashMap::new(),
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
