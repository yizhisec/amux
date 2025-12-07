//! Dialog input handling (confirmations, text input overlays)

use super::super::app::App;
use super::super::state::AsyncAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle input when in confirm delete mode
pub fn handle_confirm_delete_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        // Confirm with y or Enter
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            Some(AsyncAction::ConfirmDelete)
        }
        // Cancel with n, N, or Esc
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
            None
        }
        _ => None,
    }
}

/// Handle input when in confirm delete branch mode (after worktree deletion)
pub fn handle_confirm_delete_branch_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        // Confirm with y - delete the branch
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(AsyncAction::ConfirmDeleteBranch),
        // Cancel with n, N, or Esc - keep the branch
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
            None
        }
        _ => None,
    }
}

/// Handle input when in confirm delete worktree sessions mode
pub fn handle_confirm_delete_worktree_sessions_sync(
    app: &mut App,
    key: KeyEvent,
) -> Option<AsyncAction> {
    match key.code {
        // Confirm with y or Enter - delete sessions and proceed to worktree deletion
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            Some(AsyncAction::ConfirmDeleteWorktreeSessions)
        }
        // Cancel with n, N, or Esc
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
            None
        }
        _ => None,
    }
}

/// Handle input when in add worktree mode
pub fn handle_add_worktree_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Shift+Enter: insert newline (when typing new branch name)
    if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
        app.input_buffer.push('\n');
        return None;
    }

    match key.code {
        // Cancel
        KeyCode::Esc => {
            app.cancel_input();
            None
        }
        // Confirm selection
        KeyCode::Enter => Some(AsyncAction::SubmitAddWorktree),
        // Navigate up in branch list (clear input buffer if typing)
        KeyCode::Up | KeyCode::Char('k') if app.input_buffer.is_empty() => {
            if app.add_worktree_idx > 0 {
                app.add_worktree_idx -= 1;
            }
            None
        }
        // Navigate down in branch list
        KeyCode::Down | KeyCode::Char('j') if app.input_buffer.is_empty() => {
            if app.add_worktree_idx + 1 < app.available_branches.len() {
                app.add_worktree_idx += 1;
            }
            None
        }
        // Backspace - delete character or if empty, go back to list selection
        KeyCode::Backspace => {
            app.input_buffer.pop();
            None
        }
        // Type character - switch to new branch input mode
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}

/// Handle input when in rename session mode
pub fn handle_rename_session_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match (key.code, key.modifiers) {
        // Shift+Enter: insert newline
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            app.input_buffer.push('\n');
            None
        }
        (KeyCode::Enter, _) => Some(AsyncAction::SubmitRenameSession),
        (KeyCode::Esc, _) => {
            app.cancel_input();
            None
        }
        (KeyCode::Backspace, _) => {
            app.input_buffer.pop();
            None
        }
        (KeyCode::Char(c), _) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}

/// Handle input when adding a line comment
pub fn handle_add_line_comment_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match (key.code, key.modifiers) {
        // Shift+Enter: insert newline
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            app.input_buffer.push('\n');
            None
        }
        (KeyCode::Enter, _) => Some(AsyncAction::SubmitLineComment),
        (KeyCode::Esc, _) => {
            app.cancel_input();
            None
        }
        (KeyCode::Backspace, _) => {
            app.input_buffer.pop();
            None
        }
        (KeyCode::Char(c), _) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}

/// Handle input when editing a line comment
pub fn handle_edit_line_comment_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match (key.code, key.modifiers) {
        // Shift+Enter: insert newline
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            app.input_buffer.push('\n');
            None
        }
        (KeyCode::Enter, _) => Some(AsyncAction::UpdateLineComment),
        (KeyCode::Esc, _) => {
            app.cancel_input();
            None
        }
        (KeyCode::Backspace, _) => {
            app.input_buffer.pop();
            None
        }
        (KeyCode::Char(c), _) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}

/// Handle input when in text entry mode (new branch name)
pub fn handle_input_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match (key.code, key.modifiers) {
        // Shift+Enter: insert newline
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            app.input_buffer.push('\n');
            None
        }
        (KeyCode::Enter, _) => Some(AsyncAction::SubmitInput),
        (KeyCode::Esc, _) => {
            app.cancel_input();
            None
        }
        (KeyCode::Backspace, _) => {
            app.input_buffer.pop();
            None
        }
        (KeyCode::Char(c), _) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}
