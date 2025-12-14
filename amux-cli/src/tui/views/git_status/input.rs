//! Git status panel input handling

use crate::tui::app::App;
use crate::tui::input::resolver;
use crate::tui::state::{AsyncAction, Focus, RightPanelView};
use amux_config::Action;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle input in git status panel
pub fn handle_git_status_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Try to resolve the key to an action using the git_status context
    if let Some(pattern_str) = resolver::key_event_to_pattern_string(key) {
        if let Some(action) = app
            .keybinds
            .resolve(&pattern_str, amux_config::BindingContext::GitStatus)
        {
            return execute_git_status_action(app, action);
        }
    }

    // Fallback for keys not in keybinds
    match key.code {
        // Tab: switch to diff view showing selected file
        KeyCode::Tab => {
            if let Some(file_path) = app.current_git_file_path() {
                app.status_message = Some(format!("Opening diff for: {}", file_path));
                // Store file path to auto-expand after loading
                if let Some(git) = app.git_mut() {
                    git.pending_diff_file = Some(file_path);
                }
            } else {
                app.status_message = Some("Switching to Diff panel".to_string());
            }
            app.right_panel_view = RightPanelView::Diff;
            app.focus = Focus::DiffFiles;
            Some(AsyncAction::LoadDiffFiles)
        }

        // Back to previous focus
        KeyCode::Esc => {
            if !app.restore_focus() {
                app.focus = Focus::Sidebar;
            }
            None
        }

        _ => None,
    }
}

/// Execute a git status action
fn execute_git_status_action(app: &mut App, action: Action) -> Option<AsyncAction> {
    match action {
        Action::FocusSidebar | Action::ClosePopup => {
            if !app.restore_focus() {
                app.focus = Focus::Sidebar;
            }
            None
        }

        Action::MoveUp => {
            app.git_status_move_up();
            None
        }

        Action::MoveDown => {
            app.git_status_move_down();
            None
        }

        Action::ToggleOrOpen => {
            // If on a file, open diff for that file
            if let Some(file_path) = app.current_git_file_path() {
                app.status_message = Some(format!("Opening diff for: {}", file_path));
                app.right_panel_view = RightPanelView::Diff;
                app.focus = Focus::DiffFiles;
                // Store the file path to expand after loading
                if let Some(git) = app.git_mut() {
                    git.pending_diff_file = Some(file_path);
                }
                return Some(AsyncAction::LoadDiffFiles);
            }
            // If on a section header, toggle expand/collapse
            app.toggle_git_section_expand();
            None
        }

        Action::StageFile => {
            if let Some(file_path) = app.current_git_file_path() {
                if !app.is_current_git_item_staged() {
                    return Some(AsyncAction::StageFile { file_path });
                }
            }
            None
        }

        Action::UnstageFile => {
            if let Some(file_path) = app.current_git_file_path() {
                if app.is_current_git_item_staged() {
                    return Some(AsyncAction::UnstageFile { file_path });
                }
            }
            None
        }

        Action::StageAll => Some(AsyncAction::StageAll),
        Action::UnstageAll => Some(AsyncAction::UnstageAll),

        Action::GitPush => Some(AsyncAction::GitPush),
        Action::GitPull => Some(AsyncAction::GitPull),

        Action::RefreshStatus => Some(AsyncAction::LoadGitStatus),

        // Unhandled or context-inappropriate actions
        _ => None,
    }
}
