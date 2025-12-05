//! Input handling - navigation mode vs terminal Normal/Insert modes
//!
//! Supports both direct keybindings and prefix key mode (Ctrl+s as prefix).
//! Prefix mode allows access to navigation commands from any context.

use super::app::{App, Focus, InputMode, PrefixMode, TerminalMode};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle keyboard input
pub async fn handle_input(app: &mut App, key: KeyEvent) -> Result<()> {
    // Check for prefix key (Ctrl+s) - works in any context except text input
    if is_prefix_key(&key) && !is_text_input_mode(app) {
        app.prefix_mode = PrefixMode::WaitingForCommand;
        return Ok(());
    }

    // Handle prefix mode commands
    if app.prefix_mode == PrefixMode::WaitingForCommand {
        return handle_prefix_command(app, key).await;
    }

    // Handle input mode (new branch name entry)
    if app.input_mode == InputMode::NewBranch {
        return handle_input_mode(app, key).await;
    }

    // Handle add worktree mode
    if app.input_mode == InputMode::AddWorktree {
        return handle_add_worktree_mode(app, key).await;
    }

    // Handle confirm delete mode
    if matches!(app.input_mode, InputMode::ConfirmDelete(_)) {
        return handle_confirm_delete(app, key).await;
    }

    // Handle confirm delete branch mode
    if matches!(app.input_mode, InputMode::ConfirmDeleteBranch(_)) {
        return handle_confirm_delete_branch(app, key).await;
    }

    // Handle confirm delete worktree sessions mode
    if matches!(app.input_mode, InputMode::ConfirmDeleteWorktreeSessions { .. }) {
        return handle_confirm_delete_worktree_sessions(app, key).await;
    }

    // Handle terminal modes when focused on terminal
    if app.focus == Focus::Terminal {
        return match app.terminal_mode {
            TerminalMode::Insert => handle_insert_mode(app, key).await,
            TerminalMode::Normal => handle_terminal_normal_mode(app, key).await,
        };
    }

    // Handle sidebar navigation
    handle_navigation_input(app, key).await
}

/// Check if key is the prefix key (Ctrl+s)
fn is_prefix_key(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s')
}

/// Check if we're in a text input mode where prefix key shouldn't work
fn is_text_input_mode(app: &App) -> bool {
    matches!(app.input_mode, InputMode::NewBranch | InputMode::AddWorktree)
}

/// Handle commands after prefix key (Ctrl+s + ?)
async fn handle_prefix_command(app: &mut App, key: KeyEvent) -> Result<()> {
    // Reset prefix mode first
    app.prefix_mode = PrefixMode::None;

    // Esc cancels prefix mode
    if key.code == KeyCode::Esc {
        return Ok(());
    }

    match key.code {
        // Navigation: b = go to Branches, s = go to Sessions
        KeyCode::Char('b') => {
            // Exit terminal if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Branches;
        }
        KeyCode::Char('s') => {
            // Exit terminal if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Sessions;
        }

        // Terminal: t = go to Terminal (enter insert mode)
        KeyCode::Char('t') => {
            if app.active_session_id.is_some() {
                app.enter_terminal().await?;
            }
        }

        // Actions: n = new (session/worktree based on context)
        KeyCode::Char('n') => {
            // Exit terminal first if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.create_new().await?;
        }

        // Actions: a = add worktree
        KeyCode::Char('a') => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Branches;
            app.start_add_worktree();
        }

        // Actions: d = delete
        KeyCode::Char('d') => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.request_delete();
        }

        // Actions: r = refresh
        KeyCode::Char('r') => {
            app.refresh_all().await?;
        }

        // Actions: f = toggle fullscreen (terminal)
        KeyCode::Char('f') | KeyCode::Char('z') => {
            if app.focus == Focus::Terminal || app.active_session_id.is_some() {
                app.toggle_fullscreen();
            }
        }

        // Repo switching: 1-9
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.switch_repo(idx).await;
        }

        // Quit: q
        KeyCode::Char('q') => {
            app.should_quit = true;
        }

        // Unknown command - show hint
        _ => {
            app.status_message = Some("Prefix: b=branches s=sessions t=terminal n=new d=delete r=refresh q=quit".to_string());
        }
    }

    Ok(())
}

/// Handle input when in confirm delete mode
async fn handle_confirm_delete(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        // Confirm with y or Enter
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            app.confirm_delete().await?;
        }
        // Cancel with n, N, or Esc
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
        }
        _ => {}
    }
    Ok(())
}

/// Handle input when in confirm delete branch mode (after worktree deletion)
async fn handle_confirm_delete_branch(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        // Confirm with y - delete the branch
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.confirm_delete_branch().await?;
        }
        // Cancel with n, N, or Esc - keep the branch
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
        }
        _ => {}
    }
    Ok(())
}

/// Handle input when in confirm delete worktree sessions mode
async fn handle_confirm_delete_worktree_sessions(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        // Confirm with y or Enter - delete sessions and proceed to worktree deletion
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            app.confirm_delete_worktree_sessions().await?;
        }
        // Cancel with n, N, or Esc
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
        }
        _ => {}
    }
    Ok(())
}

/// Handle input when in add worktree mode
async fn handle_add_worktree_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        // Cancel
        KeyCode::Esc => {
            app.cancel_input();
        }
        // Confirm selection
        KeyCode::Enter => {
            app.submit_add_worktree().await?;
        }
        // Navigate up in branch list (clear input buffer if typing)
        KeyCode::Up | KeyCode::Char('k') if app.input_buffer.is_empty() => {
            if app.add_worktree_idx > 0 {
                app.add_worktree_idx -= 1;
            }
        }
        // Navigate down in branch list
        KeyCode::Down | KeyCode::Char('j') if app.input_buffer.is_empty() => {
            if app.add_worktree_idx + 1 < app.available_branches.len() {
                app.add_worktree_idx += 1;
            }
        }
        // Backspace - delete character or if empty, go back to list selection
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        // Type character - switch to new branch input mode
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        _ => {}
    }
    Ok(())
}

/// Handle input when in text entry mode (new branch name)
async fn handle_input_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Enter => {
            app.submit_input().await?;
        }
        KeyCode::Esc => {
            app.cancel_input();
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        _ => {}
    }
    Ok(())
}

/// Handle input in terminal Insert mode (send to PTY)
async fn handle_insert_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    // Esc exits to Normal mode
    if key.code == KeyCode::Esc {
        app.exit_to_normal_mode();
        return Ok(());
    }

    // Convert key to bytes and send to terminal
    let data = key_to_bytes(&key);
    if !data.is_empty() {
        app.send_to_terminal(data).await?;
    }

    Ok(())
}

/// Handle input in terminal Normal mode (scroll/browse)
async fn handle_terminal_normal_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        // Toggle fullscreen
        KeyCode::Char('f') | KeyCode::Char('z') => {
            app.toggle_fullscreen();
        }

        // Exit to Sessions (or exit fullscreen first)
        KeyCode::Esc | KeyCode::BackTab => {
            app.exit_terminal();
        }

        // Enter Insert mode
        KeyCode::Char('i') | KeyCode::Enter => {
            app.enter_insert_mode();
        }

        // Scroll up (show older content)
        KeyCode::Char('k') | KeyCode::Up => {
            app.scroll_up(1);
        }

        // Scroll down (show newer content)
        KeyCode::Char('j') | KeyCode::Down => {
            app.scroll_down(1);
        }

        // Half page up
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_up(10);
        }

        // Half page down
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_down(10);
        }

        // Go to bottom
        KeyCode::Char('G') => {
            app.scroll_to_bottom();
        }

        // Go to top (gg - for simplicity, just 'g' works too)
        KeyCode::Char('g') => {
            app.scroll_to_top();
        }

        _ => {}
    }

    Ok(())
}

/// Handle input in navigation mode (sidebar)
async fn handle_navigation_input(app: &mut App, key: KeyEvent) -> Result<()> {
    // Clear status messages on any key press
    app.status_message = None;

    match key.code {
        // Repo switching (1-9)
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            app.switch_repo(idx).await;
        }

        // Tab: forward navigation (Branches -> Sessions -> Terminal Normal)
        KeyCode::Tab => {
            match app.focus {
                Focus::Branches => {
                    app.focus = Focus::Sessions;
                }
                Focus::Sessions => {
                    // Enter terminal Normal mode if session is active
                    if app.active_session_id.is_some() {
                        app.enter_terminal().await?;
                    }
                }
                Focus::Terminal => {
                    // Shouldn't happen here
                }
            }
        }

        // Esc/Shift+Tab: backward navigation
        KeyCode::Esc | KeyCode::BackTab => {
            match app.focus {
                Focus::Branches => {
                    // Already at the beginning
                }
                Focus::Sessions => {
                    app.focus = Focus::Branches;
                }
                Focus::Terminal => {
                    // Handled in terminal modes
                }
            }
        }

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => {
            app.select_prev().await;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.select_next().await;
        }

        // Enter: forward navigation
        KeyCode::Enter => match app.focus {
            Focus::Branches => {
                app.focus = Focus::Sessions;
            }
            Focus::Sessions => {
                if app.active_session_id.is_some() {
                    app.enter_terminal().await?;
                }
            }
            Focus::Terminal => {}
        },

        // Create new (n for sessions, a for worktrees)
        KeyCode::Char('n') => {
            app.create_new().await?;
        }

        // Add worktree (when in Branches focus)
        KeyCode::Char('a') if app.focus == Focus::Branches => {
            app.start_add_worktree();
        }

        // Delete (with confirmation)
        KeyCode::Char('d') => {
            app.request_delete();
        }

        // Refresh
        KeyCode::Char('r') => {
            app.refresh_all().await?;
        }

        // Quit
        KeyCode::Char('q') => {
            app.should_quit = true;
        }

        _ => {}
    }

    Ok(())
}

/// Convert a key event to bytes to send to PTY
fn key_to_bytes(key: &KeyEvent) -> Vec<u8> {
    use KeyCode::*;

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let Char(c) = key.code {
            // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
            let ctrl_char = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a' - 1);
            return vec![ctrl_char];
        }
    }

    match key.code {
        Char(c) => c.to_string().into_bytes(),
        Enter => vec![b'\r'],
        Tab => vec![b'\t'],
        BackTab => b"\x1b[Z".to_vec(), // Shift+Tab escape sequence
        Backspace => vec![0x7f],
        Esc => vec![0x1b],
        Up => b"\x1b[A".to_vec(),
        Down => b"\x1b[B".to_vec(),
        Right => b"\x1b[C".to_vec(),
        Left => b"\x1b[D".to_vec(),
        Home => b"\x1b[H".to_vec(),
        End => b"\x1b[F".to_vec(),
        PageUp => b"\x1b[5~".to_vec(),
        PageDown => b"\x1b[6~".to_vec(),
        Delete => b"\x1b[3~".to_vec(),
        Insert => b"\x1b[2~".to_vec(),
        F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}
