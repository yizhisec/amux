//! Utility functions for input handling
//!
//! Contains common input handling patterns to reduce code duplication:
//! - Text input handling (Esc/Enter/Backspace/Char)
//! - Confirmation dialog handling (y/n/Esc)

use super::super::app::App;
use super::super::state::{AsyncAction, InputMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Check if key is the prefix key (Ctrl+s)
pub fn is_prefix_key(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s')
}

/// Check if we're in a text input mode where prefix key shouldn't work
pub fn is_text_input_mode(app: &App) -> bool {
    matches!(
        app.input_mode,
        InputMode::NewBranch
            | InputMode::AddWorktree { .. }
            | InputMode::RenameSession { .. }
            | InputMode::AddTodo { .. }
            | InputMode::EditTodo { .. }
            | InputMode::EditTodoDescription { .. }
    )
}

/// Convert a key event to bytes to send to PTY
pub fn key_to_bytes(key: &KeyEvent) -> Vec<u8> {
    use KeyCode::*;

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let Char(c) = key.code {
            // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
            let ctrl_char = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a' - 1);
            return vec![ctrl_char];
        }
    }

    // Shift+Enter sends newline (for multi-line input in Claude Code)
    if key.code == Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
        return vec![b'\n'];
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

// ============ Common Input Handling Patterns ============

/// Result of text input handling
pub enum TextInputResult {
    /// User pressed Esc - cancel input
    Cancel,
    /// User pressed Enter - submit with current buffer content
    Submit,
    /// Input was handled (character added/removed), no action needed
    Handled,
    /// Key was not handled by text input logic
    Unhandled,
}

/// Handle common text input keys (Esc, Enter, Backspace, Char)
///
/// Supports Shift+Enter for newline insertion.
/// Returns TextInputResult to indicate what happened.
pub fn handle_text_input(key: &KeyEvent, buffer: &mut String) -> TextInputResult {
    // Shift+Enter: insert newline
    if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
        buffer.push('\n');
        return TextInputResult::Handled;
    }

    match key.code {
        KeyCode::Esc => TextInputResult::Cancel,
        KeyCode::Enter => TextInputResult::Submit,
        KeyCode::Backspace => {
            buffer.pop();
            TextInputResult::Handled
        }
        KeyCode::Char(c) => {
            buffer.push(c);
            TextInputResult::Handled
        }
        _ => TextInputResult::Unhandled,
    }
}

/// Handle text input and map results to AsyncAction
///
/// This is a convenience wrapper for simple text input dialogs.
/// - on_cancel: called when Esc is pressed
/// - on_submit: called when Enter is pressed, returns the AsyncAction to perform
pub fn handle_text_input_with_actions<F, G>(
    app: &mut App,
    key: &KeyEvent,
    on_cancel: F,
    on_submit: G,
) -> Option<AsyncAction>
where
    F: FnOnce(&mut App),
    G: FnOnce(&mut App) -> Option<AsyncAction>,
{
    match handle_text_input(key, &mut app.input_buffer) {
        TextInputResult::Cancel => {
            on_cancel(app);
            None
        }
        TextInputResult::Submit => on_submit(app),
        TextInputResult::Handled | TextInputResult::Unhandled => None,
    }
}

/// Handle confirmation dialog (y/n/Esc)
///
/// Returns Some(action) if confirmed, calls on_cancel if cancelled, None otherwise.
pub fn handle_confirmation<F>(
    app: &mut App,
    key: &KeyEvent,
    on_cancel: F,
    on_confirm: AsyncAction,
) -> Option<AsyncAction>
where
    F: FnOnce(&mut App),
{
    match key.code {
        // Confirm with y or Y (some dialogs also accept Enter)
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(on_confirm),
        // Cancel with n, N, or Esc
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            on_cancel(app);
            None
        }
        _ => None,
    }
}

/// Handle confirmation dialog with Enter also confirming
pub fn handle_confirmation_with_enter<F>(
    app: &mut App,
    key: &KeyEvent,
    on_cancel: F,
    on_confirm: AsyncAction,
) -> Option<AsyncAction>
where
    F: FnOnce(&mut App),
{
    match key.code {
        // Confirm with y, Y, or Enter
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => Some(on_confirm),
        // Cancel with n, N, or Esc
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            on_cancel(app);
            None
        }
        _ => None,
    }
}
