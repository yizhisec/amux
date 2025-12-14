//! Runtime command parsing and validation
//!
//! This module handles parsing and validation of runtime commands like:
//! - `:set <option> <value>`
//! - `:bind [context] <key> <action>`
//! - `:unbind [context] <key>`
//! - `:source <file>`
//!
//! Full implementation in Phase 4.

use crate::Result;

/// Runtime command executed during application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    // Config commands
    Set {
        option: String,
        value: String,
    },

    // Binding commands
    Bind {
        context: Option<String>,
        key: String,
        action: String,
    },
    Unbind {
        context: Option<String>,
        key: String,
    },
    BindPrefix {
        key: String,
    },

    // File commands
    Source {
        path: String,
    },
    Write,

    // Display commands
    ShowBindings {
        context: Option<String>,
    },
    ShowOptions,

    // Execution
    Exec {
        action: String,
    },

    // Help
    Help,
}

impl RuntimeCommand {
    /// Parse command from input string
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();

        // Remove leading ':' if present
        let input = input.strip_prefix(':').unwrap_or(input);

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return Err(crate::ConfigError::Custom("Empty command".to_string()));
        }

        match parts[0] {
            "set" | "set-option" => {
                if parts.len() < 3 {
                    return Err(crate::ConfigError::Custom(
                        "Usage: set <option> <value>".to_string(),
                    ));
                }
                Ok(RuntimeCommand::Set {
                    option: parts[1].to_string(),
                    value: parts[2..].join(" "),
                })
            }
            "bind" | "bind-key" => {
                if parts.len() < 3 {
                    return Err(crate::ConfigError::Custom(
                        "Usage: bind [context] <key> <action>".to_string(),
                    ));
                }
                if parts.len() == 3 {
                    Ok(RuntimeCommand::Bind {
                        context: None,
                        key: parts[1].to_string(),
                        action: parts[2].to_string(),
                    })
                } else {
                    Ok(RuntimeCommand::Bind {
                        context: Some(parts[1].to_string()),
                        key: parts[2].to_string(),
                        action: parts[3..].join(" "),
                    })
                }
            }
            "unbind" | "unbind-key" => {
                if parts.len() < 2 {
                    return Err(crate::ConfigError::Custom(
                        "Usage: unbind [context] <key>".to_string(),
                    ));
                }
                Ok(RuntimeCommand::Unbind {
                    context: if parts.len() > 2 {
                        Some(parts[1].to_string())
                    } else {
                        None
                    },
                    key: parts.last().unwrap().to_string(),
                })
            }
            "prefix" | "set-prefix" => {
                if parts.len() < 2 {
                    return Err(crate::ConfigError::Custom(
                        "Usage: prefix <key>".to_string(),
                    ));
                }
                Ok(RuntimeCommand::BindPrefix {
                    key: parts[1].to_string(),
                })
            }
            "source" | "source-file" => {
                if parts.len() < 2 {
                    return Err(crate::ConfigError::Custom(
                        "Usage: source <file>".to_string(),
                    ));
                }
                Ok(RuntimeCommand::Source {
                    path: parts[1].to_string(),
                })
            }
            "w" | "write" => Ok(RuntimeCommand::Write),
            "list-keys" | "show-bindings" => Ok(RuntimeCommand::ShowBindings {
                context: parts.get(1).map(|s| s.to_string()),
            }),
            "show-options" => Ok(RuntimeCommand::ShowOptions),
            "exec" => {
                if parts.len() < 2 {
                    return Err(crate::ConfigError::Custom(
                        "Usage: exec <action>".to_string(),
                    ));
                }
                Ok(RuntimeCommand::Exec {
                    action: parts[1..].join(" "),
                })
            }
            "help" | "?" => Ok(RuntimeCommand::Help),
            _ => Err(crate::ConfigError::Custom(format!(
                "Unknown command: {}",
                parts[0]
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_set_command() {
        let cmd = RuntimeCommand::parse(":set mouse_enabled true").unwrap();
        assert_eq!(
            cmd,
            RuntimeCommand::Set {
                option: "mouse_enabled".to_string(),
                value: "true".to_string()
            }
        );
    }

    #[test]
    fn test_parse_bind_command() {
        let cmd = RuntimeCommand::parse(":bind j move-down").unwrap();
        assert_eq!(
            cmd,
            RuntimeCommand::Bind {
                context: None,
                key: "j".to_string(),
                action: "move-down".to_string()
            }
        );
    }
}
