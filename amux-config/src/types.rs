//! Configuration data structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Prefix key configuration
    #[serde(default)]
    pub prefix: PrefixConfig,

    /// Global options (UI, behavior, features)
    #[serde(default)]
    pub options: Options,

    /// UI-specific settings
    #[serde(default)]
    pub ui: UiConfig,

    /// All key bindings organized by context
    #[serde(default)]
    pub bindings: Bindings,

    /// AI provider configuration
    #[serde(default)]
    pub providers: ProvidersConfig,

    /// Source files to load (for modularity)
    #[serde(default)]
    pub source: Vec<String>,
}

/// AI Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    /// Default provider to use when creating sessions
    #[serde(default = "default_provider")]
    pub default: String,

    /// Claude provider settings
    #[serde(default)]
    pub claude: ClaudeConfig,

    /// Codex provider settings
    #[serde(default)]
    pub codex: CodexConfig,
}

/// Claude provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    /// Whether Claude provider is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Command path for Claude CLI
    #[serde(default = "default_claude_command")]
    pub command: String,

    /// Default model to use
    #[serde(default = "default_claude_model")]
    pub model: String,
}

/// OpenAI Codex provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexConfig {
    /// Whether Codex provider is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Command path for Codex CLI
    #[serde(default = "default_codex_command")]
    pub command: String,

    /// Default model to use
    #[serde(default = "default_codex_model")]
    pub model: String,
}

fn default_provider() -> String {
    "claude".to_string()
}

fn default_claude_command() -> String {
    "claude".to_string()
}

fn default_claude_model() -> String {
    "sonnet".to_string()
}

fn default_codex_command() -> String {
    "codex".to_string()
}

fn default_codex_model() -> String {
    "o4-mini".to_string()
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            default: default_provider(),
            claude: ClaudeConfig::default(),
            codex: CodexConfig::default(),
        }
    }
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            command: default_claude_command(),
            model: default_claude_model(),
        }
    }
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            command: default_codex_command(),
            model: default_codex_model(),
        }
    }
}

/// Prefix key configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixConfig {
    /// Prefix key string (e.g., "C-s", "C-a")
    #[serde(default = "default_prefix_key")]
    pub key: String,
}

/// Global options for application behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    /// Enable tree view for sessions
    #[serde(default = "default_true")]
    pub tree_view_enabled: bool,

    /// Enable git status panel
    #[serde(default = "default_true")]
    pub git_panel_enabled: bool,

    /// Enable mouse support
    #[serde(default)]
    pub mouse_enabled: bool,

    /// Fullscreen when connecting to session
    #[serde(default)]
    pub fullscreen_on_connect: bool,

    /// Show completed TODOs by default
    #[serde(default)]
    pub show_completed_todos: bool,
}

/// UI-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Show UI borders
    #[serde(default = "default_true")]
    pub show_borders: bool,

    /// Sidebar width in characters
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,

    /// Default terminal rows
    #[serde(default = "default_terminal_rows")]
    pub terminal_rows: u16,

    /// Default terminal columns
    #[serde(default = "default_terminal_cols")]
    pub terminal_cols: u16,

    /// Terminal scrollback buffer size
    #[serde(default = "default_scrollback")]
    pub terminal_scrollback: usize,
}

/// All key bindings organized by context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bindings {
    /// Global key bindings (no prefix, no context)
    #[serde(default)]
    pub global: HashMap<String, String>,

    /// Prefix-based bindings (require prefix key first)
    #[serde(default)]
    pub prefix: HashMap<String, String>,

    /// Sidebar/navigation context
    #[serde(default)]
    pub sidebar: HashMap<String, String>,

    /// Terminal normal mode (read-only, vim-like)
    #[serde(default)]
    pub terminal_normal: HashMap<String, String>,

    /// Terminal insert mode (forward to PTY mostly)
    #[serde(default)]
    pub terminal_insert: HashMap<String, String>,

    /// Diff view context
    #[serde(default)]
    pub diff: HashMap<String, String>,

    /// Git status panel context
    #[serde(default)]
    pub git_status: HashMap<String, String>,

    /// TODO popup context
    #[serde(default)]
    pub todo: HashMap<String, String>,

    /// Text input dialog context
    #[serde(default)]
    pub dialog_text: HashMap<String, String>,

    /// Confirmation dialog context
    #[serde(default)]
    pub dialog_confirm: HashMap<String, String>,
}

// Default value helper functions
fn default_true() -> bool {
    true
}

fn default_prefix_key() -> String {
    "C-s".to_string()
}

fn default_sidebar_width() -> u16 {
    30
}

fn default_terminal_rows() -> u16 {
    24
}

fn default_terminal_cols() -> u16 {
    80
}

fn default_scrollback() -> usize {
    10000
}

impl Default for PrefixConfig {
    fn default() -> Self {
        Self {
            key: default_prefix_key(),
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tree_view_enabled: default_true(),
            git_panel_enabled: default_true(),
            mouse_enabled: false,
            fullscreen_on_connect: false,
            show_completed_todos: false,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_borders: default_true(),
            sidebar_width: default_sidebar_width(),
            terminal_rows: default_terminal_rows(),
            terminal_cols: default_terminal_cols(),
            terminal_scrollback: default_scrollback(),
        }
    }
}
