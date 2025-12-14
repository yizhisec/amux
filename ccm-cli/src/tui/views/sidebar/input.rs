//! Navigation mode input handling (sidebar navigation)

use crate::tui::app::App;
use crate::tui::input::resolver;
use crate::tui::state::{AsyncAction, Focus, RightPanelView, SidebarItem};
use ccm_config::Action;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle input in navigation mode (sidebar)
pub fn handle_navigation_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Clear status messages on any key press
    app.status_message = None;

    // Try to resolve the key to an action using the sidebar context
    if let Some(pattern_str) = resolver::key_event_to_pattern_string(key) {
        if let Some(action) = app
            .keybinds
            .resolve(&pattern_str, ccm_config::BindingContext::Sidebar)
        {
            return execute_sidebar_action(app, action);
        }
    }

    // Fallback to direct key code matching for complex contextual behavior
    match key.code {
        // Tab: Enter terminal if on a session
        KeyCode::Tab => {
            if app.focus == Focus::Sidebar {
                if let SidebarItem::Session(_, _) = app.current_sidebar_item() {
                    if app.terminal.active_session_id.is_some() {
                        return Some(AsyncAction::ConnectStream);
                    }
                }
            }
            None
        }

        // Esc/Shift+Tab: backward navigation
        KeyCode::Esc | KeyCode::BackTab => {
            match app.focus {
                Focus::Sidebar => {
                    // Already at the beginning
                }
                Focus::GitStatus => {
                    // Go back to sidebar
                    app.focus = Focus::Sidebar;
                }
                Focus::Terminal | Focus::DiffFiles => {
                    // Handled in their respective modes
                }
            }
            None
        }

        // Enter: toggle expand or enter terminal
        KeyCode::Enter => {
            if app.focus == Focus::Sidebar {
                match app.current_sidebar_item() {
                    SidebarItem::Worktree(_) => app.toggle_sidebar_expand(),
                    SidebarItem::Session(_, _) => {
                        if app.terminal.active_session_id.is_some() {
                            Some(AsyncAction::ConnectStream)
                        } else {
                            None
                        }
                    }
                    SidebarItem::None => None,
                }
            } else {
                None
            }
        }

        _ => None,
    }
}

/// Execute a sidebar action
fn execute_sidebar_action(app: &mut App, action: Action) -> Option<AsyncAction> {
    match action {
        Action::MoveUp => app.select_prev_sync(),
        Action::MoveDown => app.select_next_sync(),

        Action::SwitchRepo(idx) => app.switch_repo_sync(idx),

        Action::Select => {
            // Select action: toggle expand for worktrees, enter terminal for sessions
            if app.focus == Focus::Sidebar {
                match app.current_sidebar_item() {
                    SidebarItem::Worktree(_) => app.toggle_sidebar_expand(),
                    SidebarItem::Session(_, _) => {
                        if app.terminal.active_session_id.is_some() {
                            Some(AsyncAction::ConnectStream)
                        } else {
                            None
                        }
                    }
                    SidebarItem::None => None,
                }
            } else {
                None
            }
        }

        Action::ToggleExpand => app.toggle_sidebar_expand(),

        Action::FocusGitStatus if app.sidebar.git_panel_enabled => {
            app.focus = Focus::GitStatus;
            app.status_message = Some("Switched to Git Status panel".to_string());
            Some(AsyncAction::LoadGitStatus)
        }

        Action::CreateSession => Some(AsyncAction::CreateSession),

        Action::AddWorktree if app.focus == Focus::Sidebar => {
            app.start_add_worktree();
            None
        }

        Action::ToggleDiffView => {
            if app.right_panel_view == RightPanelView::Diff {
                app.switch_to_terminal_view();
                None
            } else {
                Some(AsyncAction::SwitchToDiffView)
            }
        }

        Action::DeleteCurrent => {
            app.request_delete();
            None
        }

        Action::RenameSession if app.focus == Focus::Sidebar => {
            if let SidebarItem::Session(_, _) = app.current_sidebar_item() {
                app.start_rename_session();
            }
            None
        }

        Action::RefreshAll => Some(AsyncAction::RefreshAll),

        Action::Quit => {
            app.should_quit = true;
            None
        }

        // Unhandled or context-inappropriate actions
        _ => None,
    }
}
