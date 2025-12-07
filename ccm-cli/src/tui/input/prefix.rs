//! Prefix key command handling (Ctrl+s + ?)

use super::super::app::App;
use super::super::state::{AsyncAction, Focus, InputMode, PrefixMode, TerminalMode};
use crossterm::event::{KeyCode, KeyEvent};

/// Handle commands after prefix key (Ctrl+s + ?)
pub fn handle_prefix_command_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Reset prefix mode first
    app.prefix_mode = PrefixMode::None;

    // Esc cancels prefix mode
    if key.code == KeyCode::Esc {
        return None;
    }

    match key.code {
        // Navigation: b = go to Branches, s = go to Sessions
        KeyCode::Char('b') => {
            // Exit terminal if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Branches;
            None
        }
        KeyCode::Char('s') => {
            // Exit terminal if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            // Go to sidebar (tree view) or sessions (legacy view)
            app.focus = if app.sidebar.tree_view_enabled {
                Focus::Sidebar
            } else {
                Focus::Sessions
            };
            None
        }

        // Terminal: t = go to Terminal (enter insert mode)
        KeyCode::Char('t') => {
            if app.terminal.active_session_id.is_some() {
                Some(AsyncAction::ConnectStream)
            } else {
                None
            }
        }

        // Actions: n = new (session/worktree based on context)
        KeyCode::Char('n') => {
            // Exit terminal first if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            Some(AsyncAction::CreateSession)
        }

        // Actions: a = add worktree
        KeyCode::Char('a') => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Branches;
            app.start_add_worktree();
            None
        }

        // Actions: d = delete
        KeyCode::Char('d') => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.request_delete();
            None
        }

        // Actions: r = refresh
        KeyCode::Char('r') => Some(AsyncAction::RefreshAll),

        // Actions: f = toggle fullscreen (terminal)
        KeyCode::Char('f') | KeyCode::Char('z') => {
            if app.focus == Focus::Terminal || app.terminal.active_session_id.is_some() {
                app.toggle_fullscreen();
            }
            None
        }

        // Actions: [ = exit to terminal Normal mode (from Insert)
        KeyCode::Char('[') => {
            if app.focus == Focus::Terminal && app.terminal.mode == TerminalMode::Insert {
                app.terminal.mode = TerminalMode::Normal;
                app.dirty.terminal = true;
            }
            None
        }

        // Navigation: w = go to worktree/sidebar (tree view)
        KeyCode::Char('w') => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            // Go to sidebar in tree view mode
            app.focus = Focus::Sidebar;
            None
        }

        // Actions: o = open TODO list
        KeyCode::Char('o') => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.input_mode = InputMode::TodoPopup;
            Some(AsyncAction::LoadTodos)
        }

        // Repo switching: 1-9
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.switch_repo_sync(idx)
        }

        // Quit: q
        KeyCode::Char('q') => {
            app.should_quit = true;
            None
        }

        // Unknown command - show hint
        _ => {
            app.status_message = Some(
                "Prefix: w=worktree s=sessions t=terminal [=normal n=new d=delete r=refresh q=quit"
                    .to_string(),
            );
            None
        }
    }
}
