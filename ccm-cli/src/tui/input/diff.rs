//! Diff view input handling

use super::super::app::App;
use super::super::state::AsyncAction;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle input in DiffFiles mode (unified file + line navigation)
pub fn handle_diff_files_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        // Navigate up (files and lines)
        KeyCode::Up | KeyCode::Char('k') => {
            app.diff_move_up();
            None
        }

        // Navigate down (files and lines)
        KeyCode::Down | KeyCode::Char('j') => {
            app.diff_move_down();
            None
        }

        // Jump to previous file
        KeyCode::Char('{') => {
            app.diff_prev_file();
            None
        }

        // Jump to next file
        KeyCode::Char('}') => {
            app.diff_next_file();
            None
        }

        // Toggle expand/collapse file diff ('o' or Enter)
        KeyCode::Enter | KeyCode::Char('o') => app.toggle_diff_expand(),

        // Add comment on current line
        KeyCode::Char('c') => {
            app.start_add_line_comment();
            None
        }

        // Edit comment on current line
        KeyCode::Char('C') => {
            app.start_edit_line_comment();
            None
        }

        // Delete comment on current line
        KeyCode::Char('x') => Some(AsyncAction::DeleteLineComment),

        // Jump to next comment
        KeyCode::Char('n') => {
            app.jump_to_next_comment();
            None
        }

        // Jump to previous comment
        KeyCode::Char('N') => {
            app.jump_to_prev_comment();
            None
        }

        // Submit review to Claude
        KeyCode::Char('S') => Some(AsyncAction::SubmitReviewToClaude),

        // Refresh diff
        KeyCode::Char('r') => Some(AsyncAction::LoadDiffFiles),

        // Toggle fullscreen
        KeyCode::Char('f') | KeyCode::Char('z') => {
            app.toggle_diff_fullscreen();
            None
        }

        // Back to terminal view
        KeyCode::Esc | KeyCode::Char('t') => {
            app.switch_to_terminal_view();
            None
        }

        _ => None,
    }
}
