//! Action definitions and parsing
//!
//! An Action represents a bindable command that can be executed when a key is pressed.
//! This decouples key codes from actions, allowing users to customize keybindings.

use std::str::FromStr;

/// All bindable actions in CCMan
///
/// These represent high-level commands that can be bound to keys.
/// The actual execution is done in the TUI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Navigation
    FocusSidebar,
    FocusSessions,
    FocusBranches,
    FocusTerminal,
    FocusGitStatus,
    FocusDiff,
    FocusNext,
    FocusPrev,

    // Movement (context-sensitive)
    MoveUp,
    MoveDown,
    GotoTop,
    GotoBottom,
    ToggleExpand,
    Select,

    // Terminal modes
    InsertMode,
    NormalMode,

    // Scrolling
    ScrollUp,
    ScrollDown,
    ScrollHalfPageUp,
    ScrollHalfPageDown,
    ScrollTop,
    ScrollBottom,

    // Session management
    CreateSession,
    RenameSession,
    DeleteCurrent,
    SwitchToShell,

    // Worktree
    AddWorktree,

    // Diff
    ToggleDiffView,
    PrevFile,
    NextFile,
    AddComment,
    EditComment,
    DeleteComment,
    NextComment,
    PrevComment,
    SubmitReviewClaude,

    // Git status
    StageFile,
    UnstageFile,
    StageAll,
    UnstageAll,
    ToggleOrOpen,

    // TODO
    AddTodo,
    AddChildTodo,
    EditTodoTitle,
    EditTodoDescription,
    DeleteTodo,
    ToggleTodoComplete,
    MoveTodoDown,
    MoveTodoUp,
    IndentTodo,
    DedentTodo,
    ToggleShowCompleted,

    // General
    RefreshAll,
    RefreshDiff,
    RefreshStatus,
    RefreshTodos,
    ToggleFullscreen,
    ExitFullscreen,
    ExitTerminal,
    BackToTerminal,
    ToggleTreeView,
    OpenTodo,
    ClosePopup,
    Quit,
    ShowHelp,

    // Dialog
    Submit,
    Cancel,
    Confirm,
    InsertNewline,

    // Command mode
    EnterCommandMode,

    // Special
    Noop,           // Do nothing
    SendToTerminal, // Forward to PTY
}

impl Action {
    #[allow(clippy::should_implement_trait)]
    /// Parse action from string (case-insensitive, supports aliases)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            // Navigation
            "focus-sidebar" | "focus-worktree" => Some(Action::FocusSidebar),
            "focus-sessions" => Some(Action::FocusSessions),
            "focus-branches" => Some(Action::FocusBranches),
            "focus-terminal" => Some(Action::FocusTerminal),
            "focus-git-status" | "focus-git" => Some(Action::FocusGitStatus),
            "focus-diff" => Some(Action::FocusDiff),
            "focus-next" => Some(Action::FocusNext),
            "focus-prev" => Some(Action::FocusPrev),

            // Movement
            "move-up" | "up" => Some(Action::MoveUp),
            "move-down" | "down" => Some(Action::MoveDown),
            "goto-top" | "top" => Some(Action::GotoTop),
            "goto-bottom" | "bottom" => Some(Action::GotoBottom),
            "toggle-expand" | "expand" => Some(Action::ToggleExpand),
            "select" => Some(Action::Select),

            // Terminal modes
            "insert-mode" | "insert" => Some(Action::InsertMode),
            "normal-mode" | "terminal-normal-mode" => Some(Action::NormalMode),

            // Scrolling
            "scroll-up" => Some(Action::ScrollUp),
            "scroll-down" => Some(Action::ScrollDown),
            "scroll-half-page-up" => Some(Action::ScrollHalfPageUp),
            "scroll-half-page-down" => Some(Action::ScrollHalfPageDown),
            "scroll-top" => Some(Action::ScrollTop),
            "scroll-bottom" => Some(Action::ScrollBottom),

            // Session management
            "create-session" | "new-session" => Some(Action::CreateSession),
            "rename-session" => Some(Action::RenameSession),
            "delete-current" | "delete" => Some(Action::DeleteCurrent),
            "switch-to-shell" => Some(Action::SwitchToShell),

            // Worktree
            "add-worktree" => Some(Action::AddWorktree),

            // Diff
            "toggle-diff-view" | "diff" => Some(Action::ToggleDiffView),
            "prev-file" => Some(Action::PrevFile),
            "next-file" => Some(Action::NextFile),
            "add-comment" => Some(Action::AddComment),
            "edit-comment" => Some(Action::EditComment),
            "delete-comment" => Some(Action::DeleteComment),
            "next-comment" => Some(Action::NextComment),
            "prev-comment" => Some(Action::PrevComment),
            "submit-review-claude" => Some(Action::SubmitReviewClaude),

            // Git status
            "stage-file" | "stage" => Some(Action::StageFile),
            "unstage-file" | "unstage" => Some(Action::UnstageFile),
            "stage-all" => Some(Action::StageAll),
            "unstage-all" => Some(Action::UnstageAll),
            "toggle-or-open" => Some(Action::ToggleOrOpen),

            // TODO
            "add-todo" => Some(Action::AddTodo),
            "add-child-todo" => Some(Action::AddChildTodo),
            "edit-title" | "edit-todo-title" => Some(Action::EditTodoTitle),
            "edit-description" | "edit-todo-description" => Some(Action::EditTodoDescription),
            "delete-todo" => Some(Action::DeleteTodo),
            "toggle-complete" | "toggle-todo-complete" => Some(Action::ToggleTodoComplete),
            "move-todo-down" => Some(Action::MoveTodoDown),
            "move-todo-up" => Some(Action::MoveTodoUp),
            "indent-todo" => Some(Action::IndentTodo),
            "dedent-todo" => Some(Action::DedentTodo),
            "toggle-completed" | "toggle-show-completed" => Some(Action::ToggleShowCompleted),

            // General
            "refresh-all" | "refresh" => Some(Action::RefreshAll),
            "refresh-diff" => Some(Action::RefreshDiff),
            "refresh-status" => Some(Action::RefreshStatus),
            "refresh-todos" => Some(Action::RefreshTodos),
            "toggle-fullscreen" | "fullscreen" => Some(Action::ToggleFullscreen),
            "exit-fullscreen" => Some(Action::ExitFullscreen),
            "exit-terminal" => Some(Action::ExitTerminal),
            "back-to-terminal" => Some(Action::BackToTerminal),
            "toggle-tree-view" => Some(Action::ToggleTreeView),
            "open-todo" => Some(Action::OpenTodo),
            "close-popup" => Some(Action::ClosePopup),
            "quit" | "exit" => Some(Action::Quit),
            "show-help" | "help" | "?" => Some(Action::ShowHelp),

            // Dialog
            "submit" => Some(Action::Submit),
            "cancel" => Some(Action::Cancel),
            "confirm" => Some(Action::Confirm),
            "insert-newline" => Some(Action::InsertNewline),

            // Command mode
            "command-mode" | ":" => Some(Action::EnterCommandMode),

            // Special
            "noop" | "none" => Some(Action::Noop),
            "send-to-terminal" => Some(Action::SendToTerminal),

            _ => None,
        }
    }

    /// Get display name for this action
    pub fn display_name(&self) -> &'static str {
        match self {
            Action::FocusSidebar => "Focus Sidebar",
            Action::FocusSessions => "Focus Sessions",
            Action::FocusBranches => "Focus Branches",
            Action::FocusTerminal => "Focus Terminal",
            Action::FocusGitStatus => "Focus Git Status",
            Action::FocusDiff => "Focus Diff",
            Action::FocusNext => "Focus Next",
            Action::FocusPrev => "Focus Previous",
            Action::MoveUp => "Move Up",
            Action::MoveDown => "Move Down",
            Action::GotoTop => "Goto Top",
            Action::GotoBottom => "Goto Bottom",
            Action::ToggleExpand => "Toggle Expand",
            Action::Select => "Select",
            Action::InsertMode => "Insert Mode",
            Action::NormalMode => "Normal Mode",
            Action::ScrollUp => "Scroll Up",
            Action::ScrollDown => "Scroll Down",
            Action::ScrollHalfPageUp => "Scroll Half Page Up",
            Action::ScrollHalfPageDown => "Scroll Half Page Down",
            Action::ScrollTop => "Scroll Top",
            Action::ScrollBottom => "Scroll Bottom",
            Action::CreateSession => "Create Session",
            Action::RenameSession => "Rename Session",
            Action::DeleteCurrent => "Delete Current",
            Action::SwitchToShell => "Switch to Shell",
            Action::AddWorktree => "Add Worktree",
            Action::ToggleDiffView => "Toggle Diff View",
            Action::PrevFile => "Previous File",
            Action::NextFile => "Next File",
            Action::AddComment => "Add Comment",
            Action::EditComment => "Edit Comment",
            Action::DeleteComment => "Delete Comment",
            Action::NextComment => "Next Comment",
            Action::PrevComment => "Previous Comment",
            Action::SubmitReviewClaude => "Submit Review to Claude",
            Action::StageFile => "Stage File",
            Action::UnstageFile => "Unstage File",
            Action::StageAll => "Stage All",
            Action::UnstageAll => "Unstage All",
            Action::ToggleOrOpen => "Toggle or Open",
            Action::AddTodo => "Add Todo",
            Action::AddChildTodo => "Add Child Todo",
            Action::EditTodoTitle => "Edit Todo Title",
            Action::EditTodoDescription => "Edit Todo Description",
            Action::DeleteTodo => "Delete Todo",
            Action::ToggleTodoComplete => "Toggle Todo Complete",
            Action::MoveTodoDown => "Move Todo Down",
            Action::MoveTodoUp => "Move Todo Up",
            Action::IndentTodo => "Indent Todo",
            Action::DedentTodo => "Dedent Todo",
            Action::ToggleShowCompleted => "Toggle Show Completed",
            Action::RefreshAll => "Refresh All",
            Action::RefreshDiff => "Refresh Diff",
            Action::RefreshStatus => "Refresh Status",
            Action::RefreshTodos => "Refresh Todos",
            Action::ToggleFullscreen => "Toggle Fullscreen",
            Action::ExitFullscreen => "Exit Fullscreen",
            Action::ExitTerminal => "Exit Terminal",
            Action::BackToTerminal => "Back to Terminal",
            Action::ToggleTreeView => "Toggle Tree View",
            Action::OpenTodo => "Open Todo",
            Action::ClosePopup => "Close Popup",
            Action::Quit => "Quit",
            Action::ShowHelp => "Show Help",
            Action::Submit => "Submit",
            Action::Cancel => "Cancel",
            Action::Confirm => "Confirm",
            Action::InsertNewline => "Insert Newline",
            Action::EnterCommandMode => "Enter Command Mode",
            Action::Noop => "No Operation",
            Action::SendToTerminal => "Send to Terminal",
        }
    }
}

impl FromStr for Action {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Action::from_str(s).ok_or_else(|| format!("Unknown action: {}", s))
    }
}
