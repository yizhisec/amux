//! Prefix key command handling (Ctrl+s + ?)

use super::super::app::App;
use super::super::state::{
    AsyncAction, Focus, InputMode, PrefixMode, RightPanelView, TerminalMode,
};
use super::resolver;
use ccm_config::Action;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle commands after prefix key (Ctrl+s + ?)
pub fn handle_prefix_command_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Reset prefix mode first
    app.prefix_mode = PrefixMode::None;

    // Esc cancels prefix mode
    if key.code == KeyCode::Esc {
        return None;
    }

    // Get the pattern string for the key
    let pattern_str = resolver::key_event_to_pattern_string(key)?;

    // Resolve the action using the prefix context
    let action = app
        .keybinds
        .resolve(&pattern_str, ccm_config::BindingContext::Prefix)?;

    // Execute the action
    execute_prefix_action(app, action)
}

/// Execute a prefix action
fn execute_prefix_action(app: &mut App, action: Action) -> Option<AsyncAction> {
    match action {
        Action::FocusBranches => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Branches;
            None
        }

        Action::FocusSessions => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = if app.sidebar.tree_view_enabled {
                Focus::Sidebar
            } else {
                Focus::Sessions
            };
            None
        }

        Action::FocusTerminal => {
            if app.terminal.active_session_id.is_some() {
                Some(AsyncAction::ConnectStream)
            } else {
                None
            }
        }

        Action::CreateSession => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            Some(AsyncAction::CreateSession)
        }

        Action::AddWorktree => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.save_focus();
            app.focus = Focus::Branches;
            app.start_add_worktree();
            None
        }

        Action::DeleteCurrent => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            // request_delete already calls save_focus
            app.request_delete();
            None
        }

        Action::RefreshAll => Some(AsyncAction::RefreshAll),

        Action::ToggleFullscreen => {
            if app.focus == Focus::Terminal || app.terminal.active_session_id.is_some() {
                app.toggle_fullscreen();
            }
            None
        }

        Action::NormalMode => {
            if app.focus == Focus::Terminal && app.terminal.mode == TerminalMode::Insert {
                app.terminal.mode = TerminalMode::Normal;
            }
            None
        }

        Action::FocusSidebar => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Sidebar;
            None
        }

        Action::FocusGitStatus => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::GitStatus;
            Some(AsyncAction::LoadGitStatus)
        }

        Action::FocusDiff => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::DiffFiles;
            app.right_panel_view = RightPanelView::Diff;
            Some(AsyncAction::LoadDiffFiles)
        }

        Action::OpenTodo => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.save_focus();
            app.input_mode = InputMode::TodoPopup;
            Some(AsyncAction::LoadTodos)
        }

        Action::SwitchRepo(idx) => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.switch_repo_sync(idx)
        }

        Action::Quit => {
            app.should_quit = true;
            None
        }

        // Unknown or unhandled action in prefix context
        _ => {
            app.status_message = Some(
                "Prefix: w=sidebar g=git v=diff t=terminal n=new d=delete r=refresh q=quit"
                    .to_string(),
            );
            None
        }
    }
}
