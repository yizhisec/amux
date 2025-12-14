//! Diff view input handling

use crate::tui::app::App;
use crate::tui::input::resolver;
use crate::tui::state::AsyncAction;
use amux_config::Action;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle input in DiffFiles mode (unified file + line navigation)
pub fn handle_diff_files_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Try to resolve the key to an action using the diff context
    if let Some(pattern_str) = resolver::key_event_to_pattern_string(key) {
        if let Some(action) = app
            .keybinds
            .resolve(&pattern_str, amux_config::BindingContext::Diff)
        {
            return execute_diff_action(app, action);
        }
    }

    // Fallback for keys not in keybinds
    match key.code {
        // Back to terminal view
        KeyCode::Esc => {
            app.switch_to_terminal_view();
            None
        }

        _ => None,
    }
}

/// Execute a diff view action
fn execute_diff_action(app: &mut App, action: Action) -> Option<AsyncAction> {
    match action {
        Action::MoveUp => {
            app.diff_move_up();
            None
        }

        Action::MoveDown => {
            app.diff_move_down();
            None
        }

        Action::PrevFile => {
            app.diff_prev_file();
            None
        }

        Action::NextFile => {
            app.diff_next_file();
            None
        }

        Action::ToggleExpand => app.toggle_diff_expand(),

        Action::AddComment => {
            app.start_add_line_comment();
            None
        }

        Action::EditComment => {
            app.start_edit_line_comment();
            None
        }

        Action::DeleteComment => Some(AsyncAction::DeleteLineComment),

        Action::NextComment => {
            app.jump_to_next_comment();
            None
        }

        Action::PrevComment => {
            app.jump_to_prev_comment();
            None
        }

        Action::SubmitReviewClaude => Some(AsyncAction::SubmitReviewToClaude),

        Action::RefreshDiff => Some(AsyncAction::LoadDiffFiles),

        Action::ToggleFullscreen => {
            app.toggle_diff_fullscreen();
            None
        }

        Action::BackToTerminal => {
            app.switch_to_terminal_view();
            None
        }

        // Unhandled or context-inappropriate actions
        _ => None,
    }
}
