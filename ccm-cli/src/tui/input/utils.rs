//! Utility functions for input handling
//!
//! Contains common input handling patterns to reduce code duplication:
//! - Text input handling (Esc/Enter/Backspace/Char)
//! - Confirmation dialog handling (y/n/Esc)
//! - TextInput: cursor-aware text buffer with Unicode support

use super::super::app::App;
use super::super::state::{AsyncAction, InputMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;

// ============ TextInput: Cursor-aware text buffer ============

/// A cursor-aware text input buffer with Unicode support.
///
/// Handles:
/// - Cursor position tracking (in char indices, not bytes)
/// - Display width calculation for CJK/emoji characters
/// - Navigation (left/right/home/end/ctrl+arrows)
/// - Insertion/deletion at cursor position
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    /// The text content
    buffer: String,
    /// Cursor position as char index (not byte index)
    cursor: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with initial content, cursor at end
    #[allow(dead_code)]
    pub fn with_content(content: impl Into<String>) -> Self {
        let buffer: String = content.into();
        let cursor = buffer.chars().count();
        Self { buffer, cursor }
    }

    /// Get the buffer content
    pub fn content(&self) -> &str {
        &self.buffer
    }

    /// Get cursor position (char index)
    #[allow(dead_code)]
    pub fn cursor_position(&self) -> usize {
        self.cursor
    }

    /// Get display width of text before cursor (for terminal cursor positioning)
    pub fn cursor_display_offset(&self) -> usize {
        let text_before_cursor: String = self.buffer.chars().take(self.cursor).collect();
        UnicodeWidthStr::width(text_before_cursor.as_str())
    }

    /// Get total display width
    #[allow(dead_code)]
    pub fn display_width(&self) -> usize {
        UnicodeWidthStr::width(self.buffer.as_str())
    }

    /// Clear the buffer and reset cursor
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    /// Set content (cursor goes to end)
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.buffer = content.into();
        self.cursor = self.buffer.chars().count();
    }

    /// Insert character at cursor position
    pub fn insert(&mut self, c: char) {
        let byte_idx = self.cursor_to_byte_index();
        self.buffer.insert(byte_idx, c);
        self.cursor += 1;
    }

    /// Insert string at cursor position
    #[allow(dead_code)]
    pub fn insert_str(&mut self, s: &str) {
        let byte_idx = self.cursor_to_byte_index();
        self.buffer.insert_str(byte_idx, s);
        self.cursor += s.chars().count();
    }

    /// Delete character before cursor (Backspace)
    pub fn backspace(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            let byte_idx = self.cursor_to_byte_index();
            // Find the byte length of the char at cursor
            let char_len = self.buffer[byte_idx..]
                .chars()
                .next()
                .map_or(0, |c| c.len_utf8());
            self.buffer.drain(byte_idx..byte_idx + char_len);
            true
        } else {
            false
        }
    }

    /// Delete character at cursor (Delete key)
    pub fn delete(&mut self) -> bool {
        let char_count = self.buffer.chars().count();
        if self.cursor < char_count {
            let byte_idx = self.cursor_to_byte_index();
            let char_len = self.buffer[byte_idx..]
                .chars()
                .next()
                .map_or(0, |c| c.len_utf8());
            self.buffer.drain(byte_idx..byte_idx + char_len);
            true
        } else {
            false
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) -> bool {
        let char_count = self.buffer.chars().count();
        if self.cursor < char_count {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// Move cursor to start (Home)
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end (End)
    pub fn move_end(&mut self) {
        self.cursor = self.buffer.chars().count();
    }

    /// Move cursor to previous word boundary (Ctrl+Left)
    pub fn move_word_left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        let chars: Vec<char> = self.buffer.chars().collect();
        let mut pos = self.cursor;

        // Skip trailing whitespace/punctuation
        while pos > 0 && !chars[pos - 1].is_alphanumeric() {
            pos -= 1;
        }

        // Find start of word
        while pos > 0 && chars[pos - 1].is_alphanumeric() {
            pos -= 1;
        }

        if pos != self.cursor {
            self.cursor = pos;
            true
        } else {
            false
        }
    }

    /// Move cursor to next word boundary (Ctrl+Right)
    pub fn move_word_right(&mut self) -> bool {
        let chars: Vec<char> = self.buffer.chars().collect();
        let len = chars.len();

        if self.cursor >= len {
            return false;
        }

        let mut pos = self.cursor;

        // Skip current word
        while pos < len && chars[pos].is_alphanumeric() {
            pos += 1;
        }

        // Skip whitespace/punctuation
        while pos < len && !chars[pos].is_alphanumeric() {
            pos += 1;
        }

        if pos != self.cursor {
            self.cursor = pos;
            true
        } else {
            false
        }
    }

    /// Convert char index to byte index
    fn cursor_to_byte_index(&self) -> usize {
        self.buffer
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len())
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get trimmed content
    pub fn trim(&self) -> &str {
        self.buffer.trim()
    }
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

/// Handle common text input keys with full cursor support
///
/// Supports:
/// - Esc: cancel
/// - Enter: submit
/// - Shift+Enter: insert newline
/// - Backspace: delete before cursor
/// - Delete: delete at cursor
/// - Left/Right: move cursor
/// - Home/End: jump to start/end
/// - Ctrl+Left/Right: move by word
/// - Char: insert at cursor
pub fn handle_text_input(key: &KeyEvent, input: &mut TextInput) -> TextInputResult {
    // Shift+Enter: insert newline
    if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
        input.insert('\n');
        return TextInputResult::Handled;
    }

    // Handle Ctrl+arrow keys for word navigation
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Left => {
                input.move_word_left();
                return TextInputResult::Handled;
            }
            KeyCode::Right => {
                input.move_word_right();
                return TextInputResult::Handled;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => TextInputResult::Cancel,
        KeyCode::Enter => TextInputResult::Submit,
        KeyCode::Backspace => {
            input.backspace();
            TextInputResult::Handled
        }
        KeyCode::Delete => {
            input.delete();
            TextInputResult::Handled
        }
        KeyCode::Left => {
            input.move_left();
            TextInputResult::Handled
        }
        KeyCode::Right => {
            input.move_right();
            TextInputResult::Handled
        }
        KeyCode::Home => {
            input.move_home();
            TextInputResult::Handled
        }
        KeyCode::End => {
            input.move_end();
            TextInputResult::Handled
        }
        KeyCode::Char(c) => {
            input.insert(c);
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
    match handle_text_input(key, &mut app.text_input) {
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

    // ============ TextInput Unit Tests ============

    #[test]
    fn test_text_input_basic_insert() {
        let mut input = TextInput::new();
        input.insert('a');
        input.insert('b');
        assert_eq!(input.content(), "ab");
        assert_eq!(input.cursor_position(), 2);
    }

    #[test]
    fn test_text_input_cjk_display_width() {
        let mut input = TextInput::new();
        input.insert_str("Hello");
        input.insert('中'); // Chinese character - display width 2
        input.insert('文');
        assert_eq!(input.content(), "Hello中文");
        assert_eq!(input.display_width(), 5 + 2 + 2); // 9
        assert_eq!(input.cursor_display_offset(), 9);
    }

    #[test]
    fn test_text_input_cursor_movement() {
        let mut input = TextInput::with_content("Hello世界");
        assert_eq!(input.cursor_position(), 7); // 5 ASCII + 2 CJK chars

        input.move_left();
        assert_eq!(input.cursor_position(), 6);
        assert_eq!(input.cursor_display_offset(), 5 + 2); // "Hello世"

        input.move_home();
        assert_eq!(input.cursor_position(), 0);
        assert_eq!(input.cursor_display_offset(), 0);
    }

    #[test]
    fn test_text_input_backspace_cjk() {
        let mut input = TextInput::with_content("Hello世界");
        input.backspace();
        assert_eq!(input.content(), "Hello世");
        input.backspace();
        assert_eq!(input.content(), "Hello");
    }

    #[test]
    fn test_text_input_insert_at_middle() {
        let mut input = TextInput::with_content("Hello");
        input.move_home();
        input.move_right();
        input.move_right();
        // Cursor at position 2 (after "He")
        input.insert('X');
        assert_eq!(input.content(), "HeXllo");
    }

    #[test]
    fn test_text_input_delete_at_middle() {
        let mut input = TextInput::with_content("Hello");
        input.move_home();
        input.move_right();
        input.move_right();
        // Cursor at position 2 (after "He"), delete 'l'
        input.delete();
        assert_eq!(input.content(), "Helo");
    }

    #[test]
    fn test_text_input_word_navigation() {
        let mut input = TextInput::with_content("hello world test");
        input.move_home();

        input.move_word_right();
        assert_eq!(input.cursor_position(), 6); // After "hello "

        input.move_word_right();
        assert_eq!(input.cursor_position(), 12); // After "world "

        input.move_word_left();
        assert_eq!(input.cursor_position(), 6); // Back to "world"
    }

    #[test]
    fn test_text_input_clear() {
        let mut input = TextInput::with_content("test");
        input.clear();
        assert!(input.is_empty());
        assert_eq!(input.cursor_position(), 0);
    }

    #[test]
    fn test_text_input_set_content() {
        let mut input = TextInput::new();
        input.set_content("new content");
        assert_eq!(input.content(), "new content");
        assert_eq!(input.cursor_position(), 11); // cursor at end
    }

    // ============ handle_text_input Tests ============

    #[test]
    fn test_handle_text_input_character() {
        let mut input = TextInput::new();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());

        match handle_text_input(&key, &mut input) {
            TextInputResult::Handled => assert_eq!(input.content(), "a"),
            _ => panic!("Expected Handled"),
        }
    }

    #[test]
    fn test_handle_text_input_backspace() {
        let mut input = TextInput::with_content("abc");
        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());

        match handle_text_input(&key, &mut input) {
            TextInputResult::Handled => assert_eq!(input.content(), "ab"),
            _ => panic!("Expected Handled"),
        }
    }

    #[test]
    fn test_handle_text_input_esc() {
        let mut input = TextInput::with_content("text");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());

        match handle_text_input(&key, &mut input) {
            TextInputResult::Cancel => {
                // Buffer should not be cleared by the function
                assert_eq!(input.content(), "text");
            }
            _ => panic!("Expected Cancel"),
        }
    }

    #[test]
    fn test_handle_text_input_enter() {
        let mut input = TextInput::with_content("text");
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());

        match handle_text_input(&key, &mut input) {
            TextInputResult::Submit => assert_eq!(input.content(), "text"),
            _ => panic!("Expected Submit"),
        }
    }

    #[test]
    fn test_handle_text_input_shift_enter_newline() {
        let mut input = TextInput::with_content("line1");
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);

        match handle_text_input(&key, &mut input) {
            TextInputResult::Handled => assert_eq!(input.content(), "line1\n"),
            _ => panic!("Expected Handled"),
        }
    }

    #[test]
    fn test_handle_text_input_home_end() {
        let mut input = TextInput::with_content("text");
        let home_key = KeyEvent::new(KeyCode::Home, KeyModifiers::empty());
        let end_key = KeyEvent::new(KeyCode::End, KeyModifiers::empty());

        handle_text_input(&home_key, &mut input);
        assert_eq!(input.cursor_position(), 0);

        handle_text_input(&end_key, &mut input);
        assert_eq!(input.cursor_position(), 4);
    }

    #[test]
    fn test_handle_text_input_ctrl_arrows() {
        let mut input = TextInput::with_content("hello world");
        input.move_home();

        let ctrl_right = KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL);
        handle_text_input(&ctrl_right, &mut input);
        assert_eq!(input.cursor_position(), 6); // after "hello "

        let ctrl_left = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
        handle_text_input(&ctrl_left, &mut input);
        assert_eq!(input.cursor_position(), 0); // back to start
    }
}
