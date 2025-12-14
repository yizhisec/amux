//! Input handling - main input dispatcher
//!
//! Supports both direct keybindings and prefix key mode (Ctrl+s as prefix).
//! Prefix mode allows access to navigation commands from any context.
//!
//! All input handlers are synchronous and return Option<AsyncAction> for deferred execution.
//!
//! This module contains:
//! - `utils`: Utility functions (key conversion, mode checks)
//! - `prefix`: Prefix key command handling (Ctrl+s + ?)
//! - `resolver`: Key-to-action resolution
//! - `mouse`: Mouse event handling
//!
//! Input handlers for specific views are in their respective view modules:
//! - `views::sidebar::input` - Sidebar navigation
//! - `views::terminal::input` - Terminal mode input
//! - `views::diff::input` - Diff view input
//! - `views::git_status::input` - Git status panel input
//! - `views::todo::input` - TODO popup input
//! - `overlays::input` - Dialogs and confirmation overlays

mod mouse;
mod prefix;
pub mod resolver;
pub mod utils;

use crate::tui::app::App;
use crate::tui::overlays::input as overlay_input;
use crate::tui::state::{AsyncAction, Focus, InputMode, PrefixMode, TerminalMode};
use crate::tui::views::{diff, git_status, sidebar, terminal, todo};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// Re-export for external use
pub use super::widgets::TextInput;
pub use mouse::handle_mouse_sync;

/// Handle keyboard input (sync version - returns async action if needed)
pub fn handle_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Ignore Ctrl+C and Ctrl+Z at TUI level (prevent accidental exit/suspend)
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('z') => return None,
            _ => {}
        }
    }

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
        return overlay_input::handle_input_mode_sync(app, key);
    }

    // Handle add worktree mode
    if matches!(app.input_mode, InputMode::AddWorktree { .. }) {
        return overlay_input::handle_add_worktree_mode_sync(app, key);
    }

    // Handle rename session mode
    if matches!(app.input_mode, InputMode::RenameSession { .. }) {
        return overlay_input::handle_rename_session_mode_sync(app, key);
    }

    // Handle confirm delete mode
    if matches!(app.input_mode, InputMode::ConfirmDelete(_)) {
        return overlay_input::handle_confirm_delete_sync(app, key);
    }

    // Handle confirm delete branch mode
    if matches!(app.input_mode, InputMode::ConfirmDeleteBranch(_)) {
        return overlay_input::handle_confirm_delete_branch_sync(app, key);
    }

    // Handle confirm delete worktree sessions mode
    if matches!(
        app.input_mode,
        InputMode::ConfirmDeleteWorktreeSessions { .. }
    ) {
        return overlay_input::handle_confirm_delete_worktree_sessions_sync(app, key);
    }

    // Handle add line comment mode
    if matches!(app.input_mode, InputMode::AddLineComment { .. }) {
        return overlay_input::handle_add_line_comment_mode_sync(app, key);
    }

    // Handle edit line comment mode
    if matches!(app.input_mode, InputMode::EditLineComment { .. }) {
        return overlay_input::handle_edit_line_comment_mode_sync(app, key);
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
    sidebar::handle_navigation_input_sync(app, key)
}
