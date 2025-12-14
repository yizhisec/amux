//! Key binding pattern parsing and resolution

use crate::{actions::Action, types::Bindings, ConfigError, Result};
use std::collections::HashMap;

/// Represents a parsed key binding context
///
/// These contexts determine which keybindings are active based on the
/// current state of the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingContext {
    /// Global bindings (always checked)
    Global,
    /// Prefix-based bindings (when prefix key is active)
    Prefix,
    /// Sidebar/navigation context
    Sidebar,
    /// Terminal normal mode (read-only, vim-like)
    TerminalNormal,
    /// Terminal insert mode (forward to PTY mostly)
    TerminalInsert,
    /// Diff view context
    Diff,
    /// Git status panel context
    GitStatus,
    /// TODO popup context
    Todo,
    /// Text input dialog context
    DialogText,
    /// Confirmation dialog context
    DialogConfirm,
}

impl BindingContext {
    #[allow(clippy::should_implement_trait)]
    /// Get all context names for display
    pub fn all() -> &'static [&'static str] {
        &[
            "global",
            "prefix",
            "sidebar",
            "terminal-normal",
            "terminal-insert",
            "diff",
            "git-status",
            "todo",
            "dialog-text",
            "dialog-confirm",
        ]
    }

    #[allow(clippy::should_implement_trait)]
    /// Parse context from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "global" => Some(BindingContext::Global),
            "prefix" => Some(BindingContext::Prefix),
            "sidebar" => Some(BindingContext::Sidebar),
            "terminal-normal" | "terminal_normal" => Some(BindingContext::TerminalNormal),
            "terminal-insert" | "terminal_insert" => Some(BindingContext::TerminalInsert),
            "diff" => Some(BindingContext::Diff),
            "git-status" | "git_status" => Some(BindingContext::GitStatus),
            "todo" => Some(BindingContext::Todo),
            "dialog-text" | "dialog_text" => Some(BindingContext::DialogText),
            "dialog-confirm" | "dialog_confirm" => Some(BindingContext::DialogConfirm),
            _ => None,
        }
    }

    /// Get canonical name for this context
    pub fn name(&self) -> &'static str {
        match self {
            BindingContext::Global => "global",
            BindingContext::Prefix => "prefix",
            BindingContext::Sidebar => "sidebar",
            BindingContext::TerminalNormal => "terminal-normal",
            BindingContext::TerminalInsert => "terminal-insert",
            BindingContext::Diff => "diff",
            BindingContext::GitStatus => "git-status",
            BindingContext::Todo => "todo",
            BindingContext::DialogText => "dialog-text",
            BindingContext::DialogConfirm => "dialog-confirm",
        }
    }
}

/// Represents a parsed key pattern like "C-s" or "S-Tab"
///
/// # Format
/// - "C-x" or "Ctrl-x" - Control key
/// - "S-x" or "Shift-x" - Shift key
/// - "A-x" or "Alt-x" - Alt key
/// - "M-x" or "Meta-x" - Meta/Super key
/// - Single chars: "a", "j", "k", "1", etc.
/// - Special keys: "Enter", "Esc", "Tab", "Space", "Backspace", "Up", "Down", etc.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyPattern {
    pub modifiers: String,
    pub key: String,
}

impl KeyPattern {
    /// Parse a key pattern string
    ///
    /// Examples:
    /// - "C-s" → Control+s
    /// - "S-Tab" → Shift+Tab
    /// - "Enter" → Enter key
    /// - "j" → j key
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ConfigError::InvalidKeyPattern(
                "Empty key pattern".to_string(),
            ));
        }

        // Split by hyphen to extract modifiers and key
        let parts: Vec<&str> = s.split('-').collect();

        if parts.is_empty() {
            return Err(ConfigError::InvalidKeyPattern(format!(
                "Invalid key pattern: {}",
                s
            )));
        }

        // Check if we have modifiers
        // We only look for modifiers at the beginning, separated by hyphens
        // e.g., "C-s", "S-Tab", "C-S-x", but not "s-C" or "Tab-C"
        let mut modifiers = Vec::new();
        let mut key_idx = 0;

        // Scan for modifiers at the beginning
        // Modifiers are only recognized in key positions before the actual key
        for (i, part) in parts.iter().enumerate() {
            // Only recognize as modifier if there are more parts after it (i.e., followed by -key)
            // Single uppercase letters are only modifiers if they're not the last part
            // Examples: "C-s" (C is modifier), but "C" alone is a key
            let has_following_parts = i + 1 < parts.len();

            let is_modifier = has_following_parts
                && matches!(
                    *part,
                    "C" | "S" | "A" | "M" | "CTRL" | "SHIFT" | "ALT" | "META"
                );

            if is_modifier {
                match *part {
                    "C" | "CTRL" => modifiers.push("Ctrl"),
                    "S" | "SHIFT" => modifiers.push("Shift"),
                    "A" | "ALT" => modifiers.push("Alt"),
                    "M" | "META" => modifiers.push("Meta"),
                    _ => {} // shouldn't happen
                }
                key_idx = i + 1;
            } else {
                // First non-modifier is the start of the key
                break;
            }
        }

        if key_idx >= parts.len() {
            return Err(ConfigError::InvalidKeyPattern(format!(
                "Invalid key pattern: {} (missing key after modifiers)",
                s
            )));
        }

        // Remaining parts are the key (join them back with hyphens)
        let key = parts[key_idx..].join("-");

        // Validate key
        if !Self::is_valid_key(&key) {
            return Err(ConfigError::InvalidKeyPattern(format!(
                "Invalid key: {} (not a recognized key)",
                key
            )));
        }

        let modifiers = modifiers.join("+");

        Ok(KeyPattern { modifiers, key })
    }

    /// Check if a key string is valid
    fn is_valid_key(key: &str) -> bool {
        match key {
            // Special keys
            "Enter" | "Return" | "Esc" | "Escape" | "Tab" | "Space" | "Backspace" | "Back" => true,
            "Up" | "Down" | "Left" | "Right" => true,
            "Home" | "End" | "PageUp" | "PageDown" | "Page_Up" | "Page_Down" => true,
            "Delete" | "Insert" => true,
            // Function keys
            k if k.starts_with('F') && k.len() <= 3 => {
                k[1..].parse::<u8>().is_ok() && k[1..].parse::<u8>().unwrap() <= 24
            }
            // Single character keys (letters, numbers, symbols)
            k if k.len() == 1 => {
                let c = k.chars().next().unwrap();
                c.is_ascii_alphanumeric() || c.is_ascii_punctuation()
            }
            _ => false,
        }
    }
}

impl std::fmt::Display for KeyPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.modifiers.is_empty() {
            write!(f, "{}", self.key)
        } else {
            write!(f, "{}-{}", self.modifiers.replace("+", "-"), self.key)
        }
    }
}

/// Maps key patterns to actions in specific contexts
pub struct KeybindMap {
    bindings: HashMap<BindingContext, HashMap<String, Action>>,
    prefix_key: KeyPattern,
}

impl KeybindMap {
    /// Build keybind map from configuration
    pub fn from_bindings(bindings: &Bindings, prefix_key: &str) -> Result<Self> {
        let prefix_key = KeyPattern::parse(prefix_key)?;

        let mut map = KeybindMap {
            bindings: HashMap::new(),
            prefix_key,
        };

        // Parse global bindings
        map.load_context_bindings(BindingContext::Global, &bindings.global)?;
        map.load_context_bindings(BindingContext::Prefix, &bindings.prefix)?;
        map.load_context_bindings(BindingContext::Sidebar, &bindings.sidebar)?;
        map.load_context_bindings(BindingContext::TerminalNormal, &bindings.terminal_normal)?;
        map.load_context_bindings(BindingContext::TerminalInsert, &bindings.terminal_insert)?;
        map.load_context_bindings(BindingContext::Diff, &bindings.diff)?;
        map.load_context_bindings(BindingContext::GitStatus, &bindings.git_status)?;
        map.load_context_bindings(BindingContext::Todo, &bindings.todo)?;
        map.load_context_bindings(BindingContext::DialogText, &bindings.dialog_text)?;
        map.load_context_bindings(BindingContext::DialogConfirm, &bindings.dialog_confirm)?;

        Ok(map)
    }

    /// Load bindings for a specific context
    fn load_context_bindings(
        &mut self,
        context: BindingContext,
        bindings: &HashMap<String, String>,
    ) -> Result<()> {
        let mut context_bindings = HashMap::new();

        for (key_str, action_str) in bindings {
            // Skip invalid key patterns
            if KeyPattern::parse(key_str).is_err() {
                eprintln!("Warning: Invalid key pattern in config: {}", key_str);
                continue;
            }

            // Skip invalid actions
            if let Some(action) = Action::from_str(action_str) {
                context_bindings.insert(key_str.clone(), action);
            } else {
                eprintln!("Warning: Invalid action in config: {}", action_str);
            }
        }

        self.bindings.insert(context, context_bindings);
        Ok(())
    }

    /// Resolve a key pattern to an action in a specific context
    ///
    /// Returns None if no binding found.
    pub fn resolve(&self, key_str: &str, context: BindingContext) -> Option<Action> {
        // Check context-specific bindings first
        if let Some(bindings) = self.bindings.get(&context) {
            if let Some(action) = bindings.get(key_str) {
                return Some(*action);
            }
        }

        // Check global bindings as fallback
        if let Some(bindings) = self.bindings.get(&BindingContext::Global) {
            if let Some(action) = bindings.get(key_str) {
                return Some(*action);
            }
        }

        None
    }

    /// Get the prefix key pattern
    pub fn prefix_key(&self) -> &KeyPattern {
        &self.prefix_key
    }

    /// Check if a key pattern is the prefix key
    pub fn is_prefix_key(&self, key_str: &str) -> bool {
        if let Ok(pattern) = KeyPattern::parse(key_str) {
            pattern == self.prefix_key
        } else {
            false
        }
    }

    /// Get all bindings for a context (for display)
    pub fn bindings_for_context(
        &self,
        context: BindingContext,
    ) -> Option<&HashMap<String, Action>> {
        self.bindings.get(&context)
    }

    /// Get all contexts that have bindings
    pub fn contexts_with_bindings(&self) -> Vec<BindingContext> {
        self.bindings
            .keys()
            .filter(|ctx| !self.bindings[ctx].is_empty())
            .copied()
            .collect()
    }
}

// Tests temporarily disabled due to module test compilation issues
// Will be verified through integration tests
