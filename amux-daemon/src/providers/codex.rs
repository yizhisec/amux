//! OpenAI Codex CLI provider implementation

use super::{
    AiProvider, ProviderConfig, ProviderError, ProviderResult, ProviderSessionInfo, SessionMode,
};
use std::ffi::CString;
use std::path::Path;

/// OpenAI Codex CLI provider
pub struct CodexProvider {
    /// Path to codex CLI (defaults to "codex" in PATH)
    command_path: String,
}

impl Default for CodexProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexProvider {
    /// Create a new CodexProvider with default settings
    pub fn new() -> Self {
        Self {
            command_path: "codex".to_string(),
        }
    }

    /// Create a CodexProvider with custom command path
    #[allow(dead_code)]
    pub fn with_command_path(path: impl Into<String>) -> Self {
        Self {
            command_path: path.into(),
        }
    }
}

impl AiProvider for CodexProvider {
    fn name(&self) -> &str {
        "codex"
    }

    fn display_name(&self) -> &str {
        "OpenAI Codex"
    }

    fn build_command(&self, config: &ProviderConfig) -> ProviderResult<(CString, Vec<CString>)> {
        let cmd = CString::new(self.command_path.clone())
            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?;

        let mut args = vec![cmd.clone()];

        match &config.session_mode {
            SessionMode::Shell => {
                // Shell mode should not use AiProvider
                return Err(ProviderError::InvalidConfig(
                    "Shell mode should not use AiProvider".to_string(),
                ));
            }

            SessionMode::New { session_id: _ } => {
                // Codex doesn't support explicit session ID on creation
                // Sessions are auto-generated

                // Add model if specified
                if let Some(ref model) = config.model {
                    args.push(
                        CString::new("--model")
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                    args.push(
                        CString::new(model.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }

                // Add prompt if specified (as positional argument)
                if let Some(ref prompt) = config.prompt {
                    args.push(
                        CString::new(prompt.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }
            }

            SessionMode::Resume { session_id } => {
                // codex resume SESSION_ID
                args.push(
                    CString::new("resume")
                        .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                );
                args.push(
                    CString::new(session_id.as_str())
                        .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                );
            }

            SessionMode::OneShot => {
                // Use codex exec for non-interactive one-shot
                args.push(
                    CString::new("exec").map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                );

                // Add model if specified
                if let Some(ref model) = config.model {
                    args.push(
                        CString::new("--model")
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                    args.push(
                        CString::new(model.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }

                // Add prompt as positional argument
                if let Some(ref prompt) = config.prompt {
                    args.push(
                        CString::new(prompt.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }
            }
        }

        Ok((cmd, args))
    }

    fn read_session_info(
        &self,
        _session_id: &str,
        _worktree_path: &Path,
    ) -> ProviderResult<Option<ProviderSessionInfo>> {
        // Codex stores sessions differently, session info reading not yet implemented
        // TODO: Implement reading from ~/.codex/ session files if available
        Ok(None)
    }

    fn available_models(&self) -> Vec<&str> {
        // OpenAI models available through Codex
        vec!["o4-mini", "o3", "gpt-4.1", "gpt-4o"]
    }

    fn default_model(&self) -> &str {
        "o4-mini"
    }

    fn supports_resume(&self) -> bool {
        true // codex resume SESSION_ID
    }

    fn has_local_sessions(&self) -> bool {
        true // Sessions stored in ~/.codex/
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = CodexProvider::new();
        assert_eq!(provider.name(), "codex");
        assert_eq!(provider.display_name(), "OpenAI Codex");
    }

    #[test]
    fn test_build_new_session_command() {
        let provider = CodexProvider::new();
        let config = ProviderConfig::new_session(None, Some("o4-mini".to_string()));

        let (cmd, args) = provider.build_command(&config).unwrap();
        assert_eq!(cmd.to_str().unwrap(), "codex");
        assert_eq!(args.len(), 3); // codex --model o4-mini
        assert_eq!(args[1].to_str().unwrap(), "--model");
        assert_eq!(args[2].to_str().unwrap(), "o4-mini");
    }

    #[test]
    fn test_build_resume_command() {
        let provider = CodexProvider::new();
        let config = ProviderConfig::resume("test-session-id".to_string());

        let (cmd, args) = provider.build_command(&config).unwrap();
        assert_eq!(cmd.to_str().unwrap(), "codex");
        assert_eq!(args.len(), 3); // codex resume test-session-id
        assert_eq!(args[1].to_str().unwrap(), "resume");
        assert_eq!(args[2].to_str().unwrap(), "test-session-id");
    }

    #[test]
    fn test_available_models() {
        let provider = CodexProvider::new();
        let models = provider.available_models();
        assert!(models.contains(&"o4-mini"));
        assert!(models.contains(&"o3"));
    }

    #[test]
    fn test_shell_mode_error() {
        let provider = CodexProvider::new();
        let config = ProviderConfig::shell();

        let result = provider.build_command(&config);
        assert!(result.is_err());
    }
}
