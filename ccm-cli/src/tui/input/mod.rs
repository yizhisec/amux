//! Input handling - navigation mode vs terminal Normal/Insert modes
//!
//! Supports both direct keybindings and prefix key mode (Ctrl+s as prefix).
//! Prefix mode allows access to navigation commands from any context.
//!
//! All input handlers are synchronous and return Option<AsyncAction> for deferred execution.
//!
//! This module is organized into sub-modules by input context:
//! - `utils`: Utility functions (key conversion, mode checks)
//! - `prefix`: Prefix key command handling (Ctrl+s + ?)
//! - `terminal`: Terminal mode input (Insert/Normal)
//! - `navigation`: Sidebar navigation
//! - `dialogs`: Confirmation dialogs and text input overlays
//! - `diff`: Diff view input handling
//! - `git_status`: Git status panel input
//! - `todo`: TODO popup and input handling
//! - `mouse`: Mouse event handling

mod dialogs;
mod diff;
mod git_status;
mod mouse;
mod navigation;
mod prefix;
mod resolver;
mod terminal;
mod todo;
pub mod utils;

use super::app::App;
use super::state::{AsyncAction, Focus, InputMode, PrefixMode, TerminalMode};
use crossterm::event::KeyEvent;

// Re-export for external use
pub use mouse::handle_mouse_sync;
pub use utils::TextInput;

/// Handle keyboard input (sync version - returns async action if needed)
pub fn handle_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Check for prefix key - works in any context except text input
    // Use the configured prefix key from keybind map instead of hardcoded
    if resolver::is_key_the_prefix(key, &app.keybinds) && !utils::is_text_input_mode(app) {
        app.prefix_mode = PrefixMode::WaitingForCommand;
        return None;
    }

    // Handle prefix mode commands
    if app.prefix_mode == PrefixMode::WaitingForCommand {
        return prefix::handle_prefix_command_sync(app, key);
    }

    // Handle input mode (new branch name entry)
    if app.input_mode == InputMode::NewBranch {
        return dialogs::handle_input_mode_sync(app, key);
    }

    // Handle add worktree mode
    if matches!(app.input_mode, InputMode::AddWorktree { .. }) {
        return dialogs::handle_add_worktree_mode_sync(app, key);
    }

    // Handle rename session mode
    if matches!(app.input_mode, InputMode::RenameSession { .. }) {
        return dialogs::handle_rename_session_mode_sync(app, key);
    }

    // Handle confirm delete mode
    if matches!(app.input_mode, InputMode::ConfirmDelete(_)) {
        return dialogs::handle_confirm_delete_sync(app, key);
    }

    // Handle confirm delete branch mode
    if matches!(app.input_mode, InputMode::ConfirmDeleteBranch(_)) {
        return dialogs::handle_confirm_delete_branch_sync(app, key);
    }

    // Handle confirm delete worktree sessions mode
    if matches!(
        app.input_mode,
        InputMode::ConfirmDeleteWorktreeSessions { .. }
    ) {
        return dialogs::handle_confirm_delete_worktree_sessions_sync(app, key);
    }

    // Handle add line comment mode
    if matches!(app.input_mode, InputMode::AddLineComment { .. }) {
        return dialogs::handle_add_line_comment_mode_sync(app, key);
    }

    // Handle edit line comment mode
    if matches!(app.input_mode, InputMode::EditLineComment { .. }) {
        return dialogs::handle_edit_line_comment_mode_sync(app, key);
    }

    // Handle TODO modes
    if app.input_mode == InputMode::TodoPopup {
        return todo::handle_todo_popup_sync(app, key);
    }

    if matches!(app.input_mode, InputMode::AddTodo { .. }) {
        return todo::handle_add_todo_mode_sync(app, key);
    }

    if matches!(app.input_mode, InputMode::EditTodo { .. }) {
        return todo::handle_edit_todo_mode_sync(app, key);
    }

    if matches!(app.input_mode, InputMode::EditTodoDescription { .. }) {
        return todo::handle_edit_todo_description_mode_sync(app, key);
    }

    if matches!(app.input_mode, InputMode::ConfirmDeleteTodo { .. }) {
        return todo::handle_confirm_delete_todo_sync(app, key);
    }

    // Handle terminal modes when focused on terminal
    if app.focus == Focus::Terminal {
        return match app.terminal.mode {
            TerminalMode::Insert => terminal::handle_insert_mode_sync(app, key),
            TerminalMode::Normal => {
                terminal::handle_terminal_normal_mode_sync(app, key);
                None
            }
        };
    }

    // Handle diff view mode
    if app.focus == Focus::DiffFiles {
        return diff::handle_diff_files_mode_sync(app, key);
    }

    // Handle git status panel
    if app.focus == Focus::GitStatus {
        return git_status::handle_git_status_input_sync(app, key);
    }

    // Handle sidebar navigation
    navigation::handle_navigation_input_sync(app, key)
}
