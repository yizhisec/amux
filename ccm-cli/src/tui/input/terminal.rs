//! Terminal mode input handling (Insert and Normal modes)

use super::super::app::App;
use super::super::state::AsyncAction;
use super::utils::key_to_bytes;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle input in terminal Insert mode (send to PTY)
pub fn handle_insert_mode_sync(_app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // ESC is sent to PTY (like Claude Code behavior)
    // Use Prefix+[ to exit to Normal mode

    // Debug: log all Ctrl key presses
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        tracing::debug!(
            "Insert mode Ctrl key: {:?}, modifiers: {:?}",
            key.code,
            key.modifiers
        );
    }

    // Intercept Ctrl+` to switch to shell session
    // Note: Ctrl+` produces NUL character (ASCII 0) in most terminals,
    // which crossterm reports as Char('\0') or Char(' ') depending on terminal.
    // We also check for Char('@') as some terminals send Ctrl+@ for Ctrl+`.
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(
            key.code,
            KeyCode::Char('`')
                | KeyCode::Char('\0')
                | KeyCode::Char(' ')
                | KeyCode::Char('@')
                | KeyCode::Null
        )
    {
        tracing::info!("SwitchToShell triggered by {:?}", key.code);
        return Some(AsyncAction::SwitchToShell);
    }

    // Convert key to bytes and send to terminal
    let data = key_to_bytes(&key);
    if !data.is_empty() {
        Some(AsyncAction::SendToTerminal { data })
    } else {
        None
    }
}

/// Handle input in terminal Normal mode (scroll/browse)
pub fn handle_terminal_normal_mode_sync(app: &mut App, key: KeyEvent) {
    match key.code {
        // Toggle fullscreen
        KeyCode::Char('f') | KeyCode::Char('z') => {
            app.toggle_fullscreen();
        }

        // Exit fullscreen or do nothing (Esc in Normal mode stays in Normal)
        // Use Prefix+s or Prefix+w to go back to sidebar
        KeyCode::Esc => {
            if app.terminal_fullscreen {
                app.terminal_fullscreen = false;
            }
            // Esc in Normal mode: stay in Normal mode (like Claude Code)
            // User can use Tab/Shift+Tab or Prefix+s/w to navigate away
        }

        // Shift+Tab: go back to sidebar
        KeyCode::BackTab => {
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

        // Half page up (Ctrl+u or u)
        KeyCode::Char('u') => {
            app.scroll_up(10);
        }

        // Half page down (Ctrl+d or d)
        KeyCode::Char('d') => {
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
}
