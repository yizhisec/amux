//! Amux Configuration System
//!
//! A standalone configuration management library for amux with support for:
//! - TOML-based configuration files (`~/.amux/config.toml`)
//! - Key binding customization with context-aware resolution
//! - Runtime command execution (`:set`, `:bind`, etc.)
//! - Configuration persistence and source file loading
//!
//! # Architecture
//!
//! This crate is independent of the TUI and can be used in other projects.
//!
//! - [`config`] - Main configuration loading/saving
//! - [`types`] - Data structures for config, options, and bindings
//! - [`actions`] - Action enum and command parsing
//! - [`keybind`] - Key pattern parsing and keybind resolution
//! - [`commands`] - Runtime command parsing and validation

pub mod actions;
pub mod commands;
pub mod config;
pub mod defaults;
pub mod keybind;
pub mod parser;
pub mod types;
pub mod writer;

// Re-export commonly used types
pub use keybind::{BindingContext, KeyPattern, KeybindMap};
pub use types::Config;
pub use types::{Bindings, Options};

pub use actions::Action;
pub use commands::RuntimeCommand;

/// Errors that can occur during config operations
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parsing error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("Invalid key pattern: {0}")]
    InvalidKeyPattern(String),

    #[error("Invalid action: {0}")]
    InvalidAction(String),

    #[error("Invalid prefix key: {0}")]
    InvalidPrefixKey(String),

    #[error("Config validation error: {0}")]
    ValidationError(String),

    #[error("Circular source file dependency detected: {0}")]
    CircularDependency(String),

    #[error("Invalid option: {0}")]
    InvalidOption(String),

    #[error("{0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
