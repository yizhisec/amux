//! Configuration loading and management

use crate::defaults;
use crate::keybind::KeybindMap;
use crate::types::Config;
use crate::{parser, Result};
use std::path::{Path, PathBuf};

/// Get the default ccm config directory
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Cannot find home directory")
        .join(".ccm")
}

/// Get the default ccm config file path
pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// Load configuration from file, or return defaults if not found
pub fn load_or_default() -> Result<Config> {
    let config_path = config_file();

    match std::fs::read_to_string(&config_path) {
        Ok(content) => {
            // Try to parse the user's config
            match parser::parse_toml(&content) {
                Ok(config) => Ok(config),
                Err(e) => {
                    eprintln!("Warning: Failed to parse config file: {}", e);
                    eprintln!("Using default configuration");
                    Ok(defaults::default_config())
                }
            }
        }
        Err(_) => {
            // Config file doesn't exist, use defaults
            Ok(defaults::default_config())
        }
    }
}

/// Load configuration from a specific file
pub fn load_from_file(path: &Path) -> Result<Config> {
    parser::load_from_file(path)
}

impl Config {
    /// Load or return defaults
    pub fn load_or_default() -> Result<Self> {
        load_or_default()
    }

    /// Load from a specific file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        load_from_file(path)
    }

    /// Build a KeybindMap from this config
    pub fn to_keybind_map(&self) -> Result<KeybindMap> {
        KeybindMap::from_bindings(&self.bindings, &self.prefix.key)
    }

    /// Apply defaults for missing values (for merging configs)
    pub fn merge_with_defaults(&mut self) {
        let defaults = defaults::default_config();

        // Prefix key - user config takes precedence
        if self.prefix.key.is_empty() {
            self.prefix.key = defaults.prefix.key;
        }

        // Merge bindings
        let default_bindings = defaults.bindings;

        // Global bindings
        if self.bindings.global.is_empty() {
            self.bindings.global = default_bindings.global;
        }

        // Prefix bindings
        if self.bindings.prefix.is_empty() {
            self.bindings.prefix = default_bindings.prefix;
        } else {
            // Merge: add missing defaults
            for (k, v) in default_bindings.prefix {
                self.bindings.prefix.entry(k).or_insert(v);
            }
        }

        // Similar for other contexts
        merge_binding_map(&mut self.bindings.sidebar, &default_bindings.sidebar);
        merge_binding_map(
            &mut self.bindings.terminal_normal,
            &default_bindings.terminal_normal,
        );
        merge_binding_map(
            &mut self.bindings.terminal_insert,
            &default_bindings.terminal_insert,
        );
        merge_binding_map(&mut self.bindings.diff, &default_bindings.diff);
        merge_binding_map(&mut self.bindings.git_status, &default_bindings.git_status);
        merge_binding_map(&mut self.bindings.todo, &default_bindings.todo);
        merge_binding_map(
            &mut self.bindings.dialog_text,
            &default_bindings.dialog_text,
        );
        merge_binding_map(
            &mut self.bindings.dialog_confirm,
            &default_bindings.dialog_confirm,
        );
    }
}

/// Merge default bindings into user bindings
fn merge_binding_map(
    user_map: &mut std::collections::HashMap<String, String>,
    defaults: &std::collections::HashMap<String, String>,
) {
    for (k, v) in defaults {
        user_map.entry(k.clone()).or_insert(v.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_or_default() {
        let config = load_or_default().unwrap();
        assert_eq!(config.prefix.key, "C-s");
    }

    #[test]
    fn test_config_has_prefix_bindings() {
        let config = defaults::default_config();
        assert!(!config.bindings.prefix.is_empty());
        assert_eq!(
            config.bindings.prefix.get("s"),
            Some(&"focus-sessions".to_string())
        );
    }

    #[test]
    fn test_config_has_sidebar_bindings() {
        let config = defaults::default_config();
        assert!(!config.bindings.sidebar.is_empty());
        assert_eq!(
            config.bindings.sidebar.get("j"),
            Some(&"move-down".to_string())
        );
        assert_eq!(
            config.bindings.sidebar.get("k"),
            Some(&"move-up".to_string())
        );
    }
}
