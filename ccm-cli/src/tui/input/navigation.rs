//! Navigation mode input handling (sidebar navigation)

use super::super::app::App;
use super::super::state::{AsyncAction, Focus, RightPanelView, SidebarItem};
use crossterm::event::{KeyCode, KeyEvent};

/// Handle input in navigation mode (sidebar)
pub fn handle_navigation_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Clear status messages on any key press
    app.status_message = None;

    match key.code {
        // Repo switching (1-9)
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            app.switch_repo_sync(idx)
        }

        // Tab: forward navigation (Sidebar/Branches -> Sessions -> Terminal Normal)
        KeyCode::Tab => {
            match app.focus {
                Focus::Sidebar => {
                    // In tree view: Enter terminal if on a session, else do nothing
                    if let SidebarItem::Session(_, _) = app.current_sidebar_item() {
                        if app.active_session_id.is_some() {
                            return Some(AsyncAction::ConnectStream);
                        }
                    }
                }
                Focus::Branches => {
                    app.focus = Focus::Sessions;
                }
                Focus::Sessions => {
                    // Enter terminal Normal mode if session is active
                    if app.active_session_id.is_some() {
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

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => app.select_prev_sync(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next_sync(),

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
                        if app.active_session_id.is_some() {
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
                if app.active_session_id.is_some() {
                    Some(AsyncAction::ConnectStream)
                } else {
                    None
                }
            }
            Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => None,
        },

        // Toggle expand in tree view (o key)
        KeyCode::Char('o') if app.focus == Focus::Sidebar => app.toggle_sidebar_expand(),

        // Toggle tree view mode (T key)
        KeyCode::Char('T') => {
            app.toggle_tree_view();
            None
        }

        // Switch to Git Status panel (g key)
        KeyCode::Char('g') if app.focus == Focus::Sidebar && app.git_panel_enabled => {
            app.focus = Focus::GitStatus;
            app.status_message = Some("Switched to Git Status panel".to_string());
            Some(AsyncAction::LoadGitStatus)
        }

        // Create new (n for sessions, a for worktrees)
        KeyCode::Char('n') => Some(AsyncAction::CreateSession),

        // Add worktree (when in Branches or Sidebar focus)
        KeyCode::Char('a') if app.focus == Focus::Branches || app.focus == Focus::Sidebar => {
            app.start_add_worktree();
            None
        }

        // Switch to diff view
        KeyCode::Char('d') => {
            if app.right_panel_view == RightPanelView::Diff {
                // Already in diff view, switch back to terminal
                app.switch_to_terminal_view();
                None
            } else {
                // Switch to diff view
                Some(AsyncAction::SwitchToDiffView)
            }
        }

        // Delete (with confirmation) - use 'x' key
        KeyCode::Char('x') => {
            app.request_delete();
            None
        }

        // Rename session (R when in Sessions or Sidebar focus with session selected)
        KeyCode::Char('R') if app.focus == Focus::Sessions => {
            app.start_rename_session();
            None
        }
        KeyCode::Char('R') if app.focus == Focus::Sidebar => {
            if let SidebarItem::Session(_, _) = app.current_sidebar_item() {
                app.start_rename_session();
            }
            None
        }

        // Refresh (lowercase r)
        KeyCode::Char('r') => Some(AsyncAction::RefreshAll),

        // Quit
        KeyCode::Char('q') => {
            app.should_quit = true;
            None
        }

        _ => None,
    }
}
