//! Git status panel input handling

use super::super::app::App;
use super::super::state::{AsyncAction, Focus, RightPanelView};
use crossterm::event::{KeyCode, KeyEvent};

/// Handle input in git status panel
pub fn handle_git_status_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        // Navigate up
        KeyCode::Up | KeyCode::Char('k') => {
            app.git_status_move_up();
            None
        }

        // Navigate down
        KeyCode::Down | KeyCode::Char('j') => {
            app.git_status_move_down();
            None
        }

        // Toggle expand/collapse section OR open file diff
        KeyCode::Enter | KeyCode::Char('o') => {
            // If on a file, open diff for that file
            if let Some(file_path) = app.current_git_file_path() {
                app.status_message = Some(format!("Opening diff for: {}", file_path));
                app.right_panel_view = RightPanelView::Diff;
                app.focus = Focus::DiffFiles;
                // Store the file path to expand after loading
                app.git.pending_diff_file = Some(file_path);
                return Some(AsyncAction::LoadDiffFiles);
            }
            // If on a section header, toggle expand/collapse
            app.toggle_git_section_expand();
            None
        }

        // Stage file (s key) - if on unstaged/untracked file
        KeyCode::Char('s') => {
            if let Some(file_path) = app.current_git_file_path() {
                if !app.is_current_git_item_staged() {
                    return Some(AsyncAction::StageFile { file_path });
                }
            }
            None
        }

        // Unstage file (u key) - if on staged file
        KeyCode::Char('u') => {
            if let Some(file_path) = app.current_git_file_path() {
                if app.is_current_git_item_staged() {
                    return Some(AsyncAction::UnstageFile { file_path });
                }
            }
            None
        }

        // Stage all (S key)
        KeyCode::Char('S') => Some(AsyncAction::StageAll),

        // Unstage all (U key)
        KeyCode::Char('U') => Some(AsyncAction::UnstageAll),

        // Refresh git status (r key)
        KeyCode::Char('r') => Some(AsyncAction::LoadGitStatus),

        // Tab: switch to diff view showing selected file
        KeyCode::Tab => {
            if let Some(file_path) = app.current_git_file_path() {
                app.status_message = Some(format!("Opening diff for: {}", file_path));
                // Store file path to auto-expand after loading
                app.git.pending_diff_file = Some(file_path);
            } else {
                app.status_message = Some("Switching to Diff panel".to_string());
            }
            app.right_panel_view = RightPanelView::Diff;
            app.focus = Focus::DiffFiles;
            Some(AsyncAction::LoadDiffFiles)
        }

        // Back to sidebar
        KeyCode::Esc => {
            app.focus = Focus::Sidebar;
            None
        }

        _ => None,
    }
}
