//! Adapter layer between ccm-config types and TUI input handling
//!
//! This module provides utilities to:
//! 1. Convert crossterm KeyEvent to ccm-config KeyPattern
//! 2. Detect the current BindingContext from app state
//! 3. Resolve keys to actions using the keybind map

#![allow(dead_code)] // Functions will be used by refactored handlers

use amux_config::{Action, BindingContext, KeybindMap};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::app::App;
use super::super::state::Focus;

/// Convert a crossterm KeyEvent to a ccm-config KeyPattern string
pub fn key_event_to_pattern_string(key: KeyEvent) -> Option<String> {
    let mut modifiers = Vec::new();

    // Check if this is an uppercase letter (Shift is implicit in the character)
    let is_uppercase_letter = matches!(key.code, KeyCode::Char(c) if c.is_ascii_uppercase());

    // Handle modifiers in the order: Shift, Control, Alt, Meta (like tmux)
    // Don't add Shift modifier for uppercase letters - it's already in the character
    if key.modifiers.contains(KeyModifiers::SHIFT) && !is_uppercase_letter {
        modifiers.push("S");
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers.push("C");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        modifiers.push("A");
    }

    let key_str = match key.code {
        KeyCode::Char(c) => {
            // For printable chars, uppercase modifiers matter
            // "s" is different from "S", "j" is different from "J"
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' => c.to_string(),
                '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '(' | ')' | '-' | '=' | '['
                | ']' | '{' | '}' | ';' | ':' | '\'' | '"' | ',' | '.' | '/' | '\\' | '|' | '?'
                | '`' | '~' | '<' | '>' => c.to_string(),
                ' ' => "Space".to_string(),
                _ => return None,
            }
        }
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => return None,
    };

    // Build the pattern string
    if modifiers.is_empty() {
        Some(key_str)
    } else {
        Some(format!("{}-{}", modifiers.join("-"), key_str))
    }
}

/// Detect the current binding context based on app state
pub fn detect_context(app: &App) -> BindingContext {
    use super::super::state::{InputMode, TerminalMode};

    // Dialog modes have highest priority
    match &app.input_mode {
        InputMode::NewBranch
        | InputMode::AddWorktree { .. }
        | InputMode::RenameSession { .. }
        | InputMode::AddLineComment { .. }
        | InputMode::EditLineComment { .. }
        | InputMode::AddTodo { .. }
        | InputMode::EditTodo { .. }
        | InputMode::EditTodoDescription { .. } => return BindingContext::DialogText,

        InputMode::ConfirmDelete(_)
        | InputMode::ConfirmDeleteBranch(_)
        | InputMode::ConfirmDeleteWorktreeSessions { .. }
        | InputMode::ConfirmDeleteTodo { .. } => return BindingContext::DialogConfirm,

        InputMode::TodoPopup => return BindingContext::Todo,

        InputMode::Normal => {}
    }

    // Terminal modes
    if app.focus == Focus::Terminal {
        return match app.terminal.mode {
            TerminalMode::Insert => BindingContext::TerminalInsert,
            TerminalMode::Normal => BindingContext::TerminalNormal,
        };
    }

    // Other focus-based contexts
    match app.focus {
        Focus::DiffFiles => BindingContext::Diff,
        Focus::GitStatus => BindingContext::GitStatus,
        _ => BindingContext::Sidebar, // Default for any sidebar-related focus
    }
}

/// Resolve a key event to an action using the keybind map
pub fn resolve_action(app: &App, key: KeyEvent, keybinds: &KeybindMap) -> Option<Action> {
    // Get the pattern string from the key event
    let pattern_str = key_event_to_pattern_string(key)?;

    // Detect the current context
    let context = detect_context(app);

    // Try to resolve the action
    keybinds.resolve(&pattern_str, context)
}

/// Check if a key event is the prefix key
pub fn is_key_the_prefix(key: KeyEvent, keybinds: &KeybindMap) -> bool {
    if let Some(pattern_str) = key_event_to_pattern_string(key) {
        keybinds.is_prefix_key(&pattern_str)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_event_to_pattern_string_single_char() {
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::empty());
        assert_eq!(key_event_to_pattern_string(key), Some("j".to_string()));
    }

    #[test]
    fn test_key_event_to_pattern_string_ctrl_s() {
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_pattern_string(key), Some("C-s".to_string()));
    }

    #[test]
    fn test_key_event_to_pattern_string_shift_tab() {
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        assert_eq!(key_event_to_pattern_string(key), Some("S-Tab".to_string()));
    }

    #[test]
    fn test_key_event_to_pattern_string_special_key() {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(key_event_to_pattern_string(key), Some("Enter".to_string()));
    }

    #[test]
    fn test_key_event_to_pattern_string_ctrl_c() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_pattern_string(key), Some("C-c".to_string()));
    }

    #[test]
    fn test_key_event_to_pattern_string_shift_ctrl_s() {
        let key = KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::SHIFT | KeyModifiers::CONTROL,
        );
        assert_eq!(key_event_to_pattern_string(key), Some("S-C-s".to_string()));
    }
}
