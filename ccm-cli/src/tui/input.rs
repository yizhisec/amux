//! Input handling - navigation mode vs terminal Normal/Insert modes

use super::app::{App, Focus, InputMode, TerminalMode};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle keyboard input
pub async fn handle_input(app: &mut App, key: KeyEvent) -> Result<()> {
    // Handle input mode (new branch name entry)
    if app.input_mode == InputMode::NewBranch {
        return handle_input_mode(app, key).await;
    }

    // Handle confirm delete mode
    if matches!(app.input_mode, InputMode::ConfirmDelete(_)) {
        return handle_confirm_delete(app, key).await;
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

        // Create new
        KeyCode::Char('n') => {
            app.create_new().await?;
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
