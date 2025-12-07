//! Configuration file writing and serialization
//!
//! Handles saving configuration back to TOML files while preserving
//! formatting and comments where possible.
//!
//! Full implementation in Phase 5.

use crate::types::Config;
use crate::Result;
use std::path::Path;

/// Save configuration to a file
pub fn save_to_file(config: &Config, path: &Path) -> Result<()> {
    let toml_string = toml::to_string_pretty(config).map_err(crate::ConfigError::TomlSerialize)?;
    std::fs::write(path, toml_string)?;
    Ok(())
}

/// Save configuration to the default config file
pub fn save_default(config: &Config) -> Result<()> {
    let config_path = crate::config::config_file();

    // Ensure config directory exists
    let config_dir = config_path.parent().unwrap();
    if !config_dir.exists() {
        std::fs::create_dir_all(config_dir)?;
    }

    save_to_file(config, &config_path)
}

#[cfg(test)]
mod tests {
    use crate::defaults;

    #[test]
    fn test_serialization() {
        let config = defaults::default_config();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("[prefix]"));
        assert!(toml_str.contains("[options]"));
        assert!(toml_str.contains("[bindings"));
    }
}
