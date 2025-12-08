//! Terminal mode input handling (Insert and Normal modes)

use crate::tui::app::App;
use crate::tui::input::resolver;
use crate::tui::input::utils::key_to_bytes;
use crate::tui::state::AsyncAction;
use ccm_config::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle input in terminal Insert mode (send to PTY)
///
/// Terminal insert mode uses a two-tier approach:
/// 1. Check if the key is bound in terminal_insert context -> execute action
/// 2. Check if it's the prefix key -> enter prefix mode
/// 3. Otherwise -> forward to PTY
pub fn handle_insert_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Try to resolve the key to an action using the terminal_insert context
    if let Some(pattern_str) = resolver::key_event_to_pattern_string(key) {
        if let Some(action) = app
            .keybinds
            .resolve(&pattern_str, ccm_config::BindingContext::TerminalInsert)
        {
            return execute_terminal_insert_action(app, action);
        }
    }

    // Debug: log all Ctrl key presses
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        tracing::debug!(
            "Insert mode Ctrl key: {:?}, modifiers: {:?}",
            key.code,
            key.modifiers
        );
    }

    // Intercept Ctrl+` to switch to shell session (special case, not in keybinds)
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

/// Execute a terminal insert mode action
fn execute_terminal_insert_action(app: &mut App, action: Action) -> Option<AsyncAction> {
    match action {
        Action::SwitchToShell => Some(AsyncAction::SwitchToShell),

        // Exit insert mode
        Action::NormalMode => {
            app.terminal.mode = crate::tui::state::TerminalMode::Normal;
            None
        }

        // Unhandled actions in insert mode - shouldn't happen
        _ => None,
    }
}

/// Handle input in terminal Normal mode (scroll/browse)
pub fn handle_terminal_normal_mode_sync(app: &mut App, key: KeyEvent) {
    // Try to resolve the key to an action using the terminal_normal context
    if let Some(pattern_str) = resolver::key_event_to_pattern_string(key) {
        if let Some(action) = app
            .keybinds
            .resolve(&pattern_str, ccm_config::BindingContext::TerminalNormal)
        {
            execute_terminal_normal_action(app, action);
            return;
        }
    }

    // Fallback for keys not in keybinds (Esc, BackTab for special navigation)
    match key.code {
        // Exit fullscreen (Esc in Normal mode stays in Normal, but exits fullscreen if active)
        KeyCode::Esc => {
            if app.terminal.fullscreen {
                app.terminal.fullscreen = false;
            }
            // Esc in Normal mode: stay in Normal mode (like Claude Code)
            // User can use Tab/Shift+Tab or Prefix+s/w to navigate away
        }

        // Shift+Tab: go back to sidebar
        KeyCode::BackTab => {
            app.exit_terminal();
        }

        _ => {}
    }
}

/// Execute a terminal normal mode action
fn execute_terminal_normal_action(app: &mut App, action: Action) {
    match action {
        Action::ToggleFullscreen => app.toggle_fullscreen(),

        Action::InsertMode => app.enter_insert_mode(),

        Action::ScrollUp => app.scroll_up(1),
        Action::ScrollDown => app.scroll_down(1),

        Action::ScrollHalfPageUp => app.scroll_up(10),
        Action::ScrollHalfPageDown => app.scroll_down(10),

        Action::ScrollTop => app.scroll_to_top(),
        Action::ScrollBottom => app.scroll_to_bottom(),

        // Unhandled or context-inappropriate actions
        _ => {}
    }
}
