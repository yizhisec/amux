//! Dialog input handling (confirmations, text input overlays)
//!
//! Uses common input handling utilities from utils module to reduce duplication.

use super::super::app::App;
use super::super::state::{AsyncAction, InputMode};
use super::utils::{
    handle_confirmation, handle_confirmation_with_enter, handle_text_input_with_actions,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle input when in confirm delete mode
pub fn handle_confirm_delete_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    use super::super::state::{DeleteTarget, ExitCleanupAction};

    // Only Session deletion has options (Destroy/Stop)
    let is_session = matches!(
        app.input_mode,
        InputMode::ConfirmDelete(DeleteTarget::Session { .. })
    );

    if !is_session {
        // For other deletions (Worktree etc.), use simple confirmation
        let target = match &app.input_mode {
            InputMode::ConfirmDelete(t) => t.clone(),
            _ => return None,
        };
        return handle_confirmation_with_enter(
            app,
            &key,
            |a| a.cancel_input(),
            AsyncAction::ConfirmDelete {
                target,
                action: ExitCleanupAction::Destroy, // Worktree can only be destroyed
            },
        );
    }

    // Session deletion: support navigation and selection
    match key.code {
        // j/k or up/down navigate options
        KeyCode::Char('j') | KeyCode::Down => {
            app.session_delete_action = match app.session_delete_action {
                ExitCleanupAction::Destroy => ExitCleanupAction::Stop,
                ExitCleanupAction::Stop => ExitCleanupAction::Stop,
            };
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.session_delete_action = match app.session_delete_action {
                ExitCleanupAction::Destroy => ExitCleanupAction::Destroy,
                ExitCleanupAction::Stop => ExitCleanupAction::Destroy,
            };
            None
        }
        // Shortcuts
        KeyCode::Char('d') => {
            app.session_delete_action = ExitCleanupAction::Destroy;
            None
        }
        KeyCode::Char('s') => {
            app.session_delete_action = ExitCleanupAction::Stop;
            None
        }
        // Enter confirms
        KeyCode::Enter => {
            let target = match &app.input_mode {
                InputMode::ConfirmDelete(t) => t.clone(),
                _ => return None,
            };
            Some(AsyncAction::ConfirmDelete {
                target,
                action: app.session_delete_action,
            })
        }
        // Esc/n cancels
        KeyCode::Esc | KeyCode::Char('n') => {
            app.cancel_input();
            None
        }
        _ => None,
    }
}

/// Handle input when in confirm delete branch mode (after worktree deletion)
pub fn handle_confirm_delete_branch_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    handle_confirmation(
        app,
        &key,
        |a| a.cancel_input(),
        AsyncAction::ConfirmDeleteBranch,
    )
}

/// Handle input when in confirm delete worktree sessions mode
pub fn handle_confirm_delete_worktree_sessions_sync(
    app: &mut App,
    key: KeyEvent,
) -> Option<AsyncAction> {
    handle_confirmation_with_enter(
        app,
        &key,
        |a| a.cancel_input(),
        AsyncAction::ConfirmDeleteWorktreeSessions,
    )
}

/// Handle input when in add worktree mode
pub fn handle_add_worktree_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Shift+Enter: insert newline (when typing new branch name)
    if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
        app.text_input.insert('\n');
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
        KeyCode::Up | KeyCode::Char('k') if app.text_input.is_empty() => {
            if app.add_worktree_idx > 0 {
                app.add_worktree_idx -= 1;
            }
            None
        }
        // Navigate down in branch list
        KeyCode::Down | KeyCode::Char('j') if app.text_input.is_empty() => {
            if app.add_worktree_idx + 1 < app.available_branches.len() {
                app.add_worktree_idx += 1;
            }
            None
        }
        // Backspace - delete character or if empty, go back to list selection
        KeyCode::Backspace => {
            app.text_input.backspace();
            None
        }
        // Type character - switch to new branch input mode
        KeyCode::Char(c) => {
            app.text_input.insert(c);
            None
        }
        _ => None,
    }
}

/// Handle input when in rename session mode
pub fn handle_rename_session_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    handle_text_input_with_actions(
        app,
        &key,
        |a| a.cancel_input(),
        |_| Some(AsyncAction::SubmitRenameSession),
    )
}

/// Handle input when adding a line comment
pub fn handle_add_line_comment_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    handle_text_input_with_actions(
        app,
        &key,
        |a| a.cancel_input(),
        |_| Some(AsyncAction::SubmitLineComment),
    )
}

/// Handle input when editing a line comment
pub fn handle_edit_line_comment_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    handle_text_input_with_actions(
        app,
        &key,
        |a| a.cancel_input(),
        |_| Some(AsyncAction::UpdateLineComment),
    )
}

/// Handle input when in text entry mode (new branch name)
pub fn handle_input_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    handle_text_input_with_actions(
        app,
        &key,
        |a| a.cancel_input(),
        |_| Some(AsyncAction::SubmitInput),
    )
}
