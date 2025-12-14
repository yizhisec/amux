//! AI Provider abstraction layer
//!
//! This module provides a trait-based abstraction for different AI CLI tools
//! (Claude, Codex, etc.), allowing the daemon to work with multiple providers.

mod claude;
mod codex;
mod mock;
mod registry;

pub use claude::ClaudeProvider;
pub use codex::CodexProvider;
pub use mock::MockProvider;
pub use registry::ProviderRegistry;

use std::ffi::CString;
use std::path::Path;

/// Session mode for AI provider
#[derive(Debug, Clone)]
pub enum SessionMode {
    /// Run plain shell (no AI)
    Shell,
    /// New session (optionally with session ID)
    New { session_id: Option<String> },
    /// Resume existing session
    Resume { session_id: String },
    /// One-shot command (no session management)
    OneShot,
}

/// Configuration for building provider command
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Session mode
    pub session_mode: SessionMode,
    /// Model to use (e.g., "opus", "sonnet", "haiku")
    pub model: Option<String>,
    /// Initial prompt (for one-shot or new session)
    pub prompt: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            session_mode: SessionMode::New { session_id: None },
            model: None,
            prompt: None,
        }
    }
}

/// Information extracted from a provider's session files
#[derive(Debug, Clone)]
pub struct ProviderSessionInfo {
    /// First user message or session description
    pub description: Option<String>,
}

/// Result type for provider operations
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Errors that can occur in provider operations
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Provider '{name}' not found. Available providers: {available}")]
    NotFound { name: String, available: String },

    #[error("Provider '{0}' is not enabled")]
    NotEnabled(String),

    #[error("Invalid model '{model}' for provider '{provider}'. Available models: {available}")]
    InvalidModel {
        provider: String,
        model: String,
        available: String,
    },

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Session file error: {0}")]
    SessionFile(String),

    #[error("Command build error: {0}")]
    CommandBuild(String),
}

impl ProviderError {
    /// Create a NotFound error with available providers list
    pub fn not_found(name: &str, available: &[&str]) -> Self {
        Self::NotFound {
            name: name.to_string(),
            available: available.join(", "),
        }
    }

    /// Create an InvalidModel error with available models list
    pub fn invalid_model(provider: &str, model: &str, available: &[&str]) -> Self {
        Self::InvalidModel {
            provider: provider.to_string(),
            model: model.to_string(),
            available: available.join(", "),
        }
    }
}

/// Validated provider + model reference
///
/// This struct ensures that both provider and model are valid at creation time,
/// providing early validation before session creation. It holds owned copies
/// of the validated names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRef {
    /// Provider name (validated to exist in registry)
    pub name: String,
    /// Model name (validated to be available for the provider)
    pub model: String,
}

impl ProviderRef {
    /// Create a new ProviderRef with validation
    ///
    /// - If provider is None, uses the registry's default provider
    /// - If model is None, uses the provider's default model
    /// - Returns error if provider doesn't exist or model is invalid
    pub fn new(
        registry: &ProviderRegistry,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> ProviderResult<Self> {
        // Get provider name (use default if not specified)
        let provider_name = provider.unwrap_or_else(|| registry.default_provider_name());

        // Validate provider exists
        let provider_impl = registry.get_or_error(provider_name)?;

        // Get model name (use provider's default if not specified)
        let model_name = model.unwrap_or_else(|| provider_impl.default_model());

        // Validate model is available for this provider
        let available_models = provider_impl.available_models();
        if !available_models.contains(&model_name) {
            return Err(ProviderError::invalid_model(
                provider_name,
                model_name,
                &available_models,
            ));
        }

        Ok(Self {
            name: provider_name.to_string(),
            model: model_name.to_string(),
        })
    }

    /// Create a ProviderRef for shell sessions (no validation needed)
    pub fn shell() -> Self {
        Self {
            name: "shell".to_string(),
            model: String::new(),
        }
    }
}

/// Trait for AI CLI providers
///
/// Each provider (Claude, Codex, etc.) implements this trait to define
/// how to build commands, read session info, and describe its capabilities.
pub trait AiProvider: Send + Sync {
    /// Provider name (e.g., "claude", "codex")
    fn name(&self) -> &str;

    /// Display name for UI (e.g., "Claude", "OpenAI Codex")
    fn display_name(&self) -> &str;

    /// Build command and arguments for spawning the AI process
    ///
    /// Returns (command, args) suitable for execvp
    fn build_command(&self, config: &ProviderConfig) -> ProviderResult<(CString, Vec<CString>)>;

    /// Read session information from provider's session files (if supported)
    ///
    /// This is used to extract session descriptions for display in the UI.
    fn read_session_info(
        &self,
        session_id: &str,
        worktree_path: &Path,
    ) -> ProviderResult<Option<ProviderSessionInfo>>;

    /// List of available models
    fn available_models(&self) -> Vec<&str>;

    /// Default model name
    fn default_model(&self) -> &str;

    /// Whether this provider supports session resumption
    fn supports_resume(&self) -> bool;

    /// Whether this provider stores session files locally
    fn has_local_sessions(&self) -> bool;
}

/// Extension trait for converting legacy ClaudeSessionMode to ProviderConfig
impl ProviderConfig {
    /// Create config for shell mode
    pub fn shell() -> Self {
        Self {
            session_mode: SessionMode::Shell,
            model: None,
            prompt: None,
        }
    }

    /// Create config for new session
    pub fn new_session(session_id: Option<String>, model: Option<String>) -> Self {
        Self {
            session_mode: SessionMode::New { session_id },
            model,
            prompt: None,
        }
    }

    /// Create config for resuming a session
    pub fn resume(session_id: String) -> Self {
        Self {
            session_mode: SessionMode::Resume { session_id },
            model: None,
            prompt: None,
        }
    }

    /// Create config for one-shot command
    pub fn one_shot(model: Option<String>, prompt: String) -> Self {
        Self {
            session_mode: SessionMode::OneShot,
            model,
            prompt: Some(prompt),
        }
    }
}
