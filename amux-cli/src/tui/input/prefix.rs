//! Prefix key command handling (Ctrl+s + ?)

use super::super::app::App;
use super::super::state::{
    AsyncAction, Focus, InputMode, PrefixMode, RightPanelView, TerminalMode,
};
use super::resolver;
use amux_config::Action;
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
        .resolve(&pattern_str, amux_config::BindingContext::Prefix)?;

    // Execute the action
    execute_prefix_action(app, action)
}

/// Execute a prefix action
fn execute_prefix_action(app: &mut App, action: Action) -> Option<AsyncAction> {
    match action {
        Action::FocusBranches | Action::FocusSessions | Action::FocusSidebar => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Sidebar;
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

        Action::SelectProviderAndCreate => {
            app.save_focus();
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Sidebar;

            // Get current repo and branch
            let (repo_id, branch) = match (
                app.current_repo().map(|r| r.info.id.clone()),
                app.current_worktree().map(|b| b.branch.clone()),
            ) {
                (Some(r), Some(b)) => (r, b),
                _ => {
                    app.status_message = Some("No worktree selected".to_string());
                    return None;
                }
            };

            app.start_select_provider(repo_id.clone(), branch.clone());
            Some(AsyncAction::FetchProviders { repo_id, branch })
        }

        Action::AddWorktree => {
            app.save_focus();
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Sidebar;
            app.start_add_worktree();
            None
        }

        Action::DeleteCurrent => {
            app.save_focus();
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
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

        Action::FocusGitStatus => {
            app.save_focus();
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::GitStatus;
            Some(AsyncAction::LoadGitStatus)
        }

        Action::FocusDiff => {
            app.save_focus();
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::DiffFiles;
            app.right_panel_view = RightPanelView::Diff;
            Some(AsyncAction::LoadDiffFiles)
        }

        Action::OpenTodo => {
            app.save_focus();
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
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
