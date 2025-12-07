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
