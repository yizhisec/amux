//! Navigation mode input handling (sidebar navigation)

use super::super::app::App;
use super::super::state::{AsyncAction, Focus, RightPanelView, SidebarItem};
use super::resolver;
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
        // Tab: forward navigation (Sidebar/Branches -> Sessions -> Terminal Normal)
        KeyCode::Tab => {
            match app.focus {
                Focus::Sidebar => {
                    // In tree view: Enter terminal if on a session, else do nothing
                    if let SidebarItem::Session(_, _) = app.current_sidebar_item() {
                        if app.terminal.active_session_id.is_some() {
                            return Some(AsyncAction::ConnectStream);
                        }
                    }
                }
                Focus::Branches => {
                    app.focus = Focus::Sessions;
                }
                Focus::Sessions => {
                    // Enter terminal Normal mode if session is active
                    if app.terminal.active_session_id.is_some() {
                        return Some(AsyncAction::ConnectStream);
                    }
                }
                Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => {
                    // Shouldn't happen here
                }
            }
            None
        }

        // Esc/Shift+Tab: backward navigation
        KeyCode::Esc | KeyCode::BackTab => {
            match app.focus {
                Focus::Sidebar => {
                    // Already at the beginning in tree view
                }
                Focus::GitStatus => {
                    // Go back to sidebar
                    app.focus = Focus::Sidebar;
                }
                Focus::Branches => {
                    // Already at the beginning
                }
                Focus::Sessions => {
                    app.focus = Focus::Branches;
                }
                Focus::Terminal | Focus::DiffFiles => {
                    // Handled in their respective modes
                }
            }
            None
        }

        // Enter: forward navigation or toggle expand
        KeyCode::Enter => match app.focus {
            Focus::Sidebar => {
                match app.current_sidebar_item() {
                    SidebarItem::Worktree(_) => {
                        // Toggle expand when on worktree
                        app.toggle_sidebar_expand()
                    }
                    SidebarItem::Session(_, _) => {
                        // Enter terminal when on session
                        if app.terminal.active_session_id.is_some() {
                            Some(AsyncAction::ConnectStream)
                        } else {
                            None
                        }
                    }
                    SidebarItem::None => None,
                }
            }
            Focus::Branches => {
                app.focus = Focus::Sessions;
                None
            }
            Focus::Sessions => {
                if app.terminal.active_session_id.is_some() {
                    Some(AsyncAction::ConnectStream)
                } else {
                    None
                }
            }
            Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => None,
        },

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
            // Select action: context-aware behavior
            match app.focus {
                Focus::Sidebar => {
                    // In sidebar: toggle expand for worktrees, enter terminal for sessions
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
                }
                Focus::Branches => {
                    // In branches: move to sessions view
                    app.focus = Focus::Sessions;
                    None
                }
                Focus::Sessions => {
                    // In sessions: enter terminal
                    if app.terminal.active_session_id.is_some() {
                        Some(AsyncAction::ConnectStream)
                    } else {
                        None
                    }
                }
                Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => None,
            }
        }

        Action::ToggleExpand => app.toggle_sidebar_expand(),

        Action::ToggleTreeView => {
            app.toggle_tree_view();
            None
        }

        Action::FocusGitStatus if app.sidebar.git_panel_enabled => {
            app.focus = Focus::GitStatus;
            app.status_message = Some("Switched to Git Status panel".to_string());
            Some(AsyncAction::LoadGitStatus)
        }

        Action::CreateSession => Some(AsyncAction::CreateSession),

        Action::AddWorktree if app.focus == Focus::Branches || app.focus == Focus::Sidebar => {
            app.start_add_worktree();
            None
        }

        Action::ToggleDiffView => {
            if app.right_panel_view == RightPanelView::Diff {
                // Already in diff view, switch back to terminal
                app.switch_to_terminal_view();
                None
            } else {
                // Switch to diff view
                Some(AsyncAction::SwitchToDiffView)
            }
        }

        Action::DeleteCurrent => {
            app.request_delete();
            None
        }

        Action::RenameSession if app.focus == Focus::Sessions => {
            app.start_rename_session();
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
