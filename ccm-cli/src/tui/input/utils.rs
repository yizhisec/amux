//! Utility functions for input handling
//!
//! Contains common input handling patterns to reduce code duplication:
//! - Text input handling (Esc/Enter/Backspace/Char)
//! - Confirmation dialog handling (y/n/Esc)

use super::super::app::App;
use super::super::state::{AsyncAction, InputMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_text_input_character() {
        let mut buffer = String::new();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());

        match handle_text_input(&key, &mut buffer) {
            TextInputResult::Handled => assert_eq!(buffer, "a"),
            _ => panic!("Expected Handled"),
        }
    }

    #[test]
    fn test_handle_text_input_multiple_characters() {
        let mut buffer = String::new();
        let key_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        let key_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::empty());

        handle_text_input(&key_a, &mut buffer);
        handle_text_input(&key_b, &mut buffer);
        assert_eq!(buffer, "ab");
    }

    #[test]
    fn test_handle_text_input_backspace() {
        let mut buffer = String::from("abc");
        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());

        match handle_text_input(&key, &mut buffer) {
            TextInputResult::Handled => assert_eq!(buffer, "ab"),
            _ => panic!("Expected Handled"),
        }
    }

    #[test]
    fn test_handle_text_input_backspace_empty() {
        let mut buffer = String::new();
        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());

        match handle_text_input(&key, &mut buffer) {
            TextInputResult::Handled => assert_eq!(buffer, ""),
            _ => panic!("Expected Handled"),
        }
    }

    #[test]
    fn test_handle_text_input_esc() {
        let mut buffer = String::from("text");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());

        match handle_text_input(&key, &mut buffer) {
            TextInputResult::Cancel => {
                // Buffer should not be cleared by the function
                assert_eq!(buffer, "text");
            }
            _ => panic!("Expected Cancel"),
        }
    }

    #[test]
    fn test_handle_text_input_enter() {
        let mut buffer = String::from("text");
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());

        match handle_text_input(&key, &mut buffer) {
            TextInputResult::Submit => assert_eq!(buffer, "text"),
            _ => panic!("Expected Submit"),
        }
    }

    #[test]
    fn test_handle_text_input_shift_enter_newline() {
        let mut buffer = String::from("line1");
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);

        match handle_text_input(&key, &mut buffer) {
            TextInputResult::Handled => assert_eq!(buffer, "line1\n"),
            _ => panic!("Expected Handled"),
        }
    }

    #[test]
    fn test_handle_text_input_unhandled_key() {
        let mut buffer = String::from("text");
        let key = KeyEvent::new(KeyCode::Home, KeyModifiers::empty());

        match handle_text_input(&key, &mut buffer) {
            TextInputResult::Unhandled => assert_eq!(buffer, "text"),
            _ => panic!("Expected Unhandled"),
        }
    }

    #[test]
    fn test_is_prefix_key_ctrl_s() {
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(is_prefix_key(&key));
    }

    #[test]
    fn test_is_prefix_key_not_ctrl_s() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(!is_prefix_key(&key));
    }

    #[test]
    fn test_is_prefix_key_s_without_ctrl() {
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty());
        assert!(!is_prefix_key(&key));
    }
}
