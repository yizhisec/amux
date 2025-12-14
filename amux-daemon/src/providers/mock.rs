//! Mock provider for testing
//!
//! This module provides a mock AI provider that can be used in tests
//! without requiring real CLI tools to be installed.

use super::{AiProvider, ProviderConfig, ProviderResult, ProviderSessionInfo};
use std::ffi::CString;
use std::path::Path;

/// A mock AI provider for testing
///
/// This provider simulates an AI CLI tool by running a simple shell command
/// that echoes output, allowing tests to verify session management and
/// PTY handling without requiring real AI tools.
pub struct MockProvider {
    name: String,
    display_name: String,
    models: Vec<String>,
    default_model: String,
}

impl MockProvider {
    /// Create a new mock provider with the given name
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            display_name: format!("Mock {}", name),
            models: vec!["mock-model-1".to_string(), "mock-model-2".to_string()],
            default_model: "mock-model-1".to_string(),
        }
    }

    /// Create with custom models
    pub fn with_models(name: &str, models: Vec<String>, default_model: String) -> Self {
        Self {
            name: name.to_string(),
            display_name: format!("Mock {}", name),
            models,
            default_model,
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new("mock")
    }
}

impl AiProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn build_command(&self, config: &ProviderConfig) -> ProviderResult<(CString, Vec<CString>)> {
        // Build a simple command that simulates AI behavior
        // Uses /bin/sh with a script that echoes responses
        let cmd = CString::new("/bin/sh").expect("Failed to create CString");

        let model = config.model.as_deref().unwrap_or(&self.default_model);
        let prompt_msg = config
            .prompt
            .as_ref()
            .map(|p| format!("Prompt: {}", p))
            .unwrap_or_else(|| "No prompt".to_string());

        // Create a simple interactive script that simulates an AI session
        let script = format!(
            r#"
echo "[MockProvider: {}]"
echo "Model: {}"
echo "{}"
echo "---"
echo "Mock AI ready. Type 'exit' to quit."
while read -r line; do
    if [ "$line" = "exit" ]; then
        exit 0
    fi
    echo "Mock response to: $line"
done
"#,
            self.name, model, prompt_msg
        );

        let args = vec![
            CString::new("-c").expect("Failed to create CString"),
            CString::new(script).expect("Failed to create CString"),
        ];

        Ok((cmd, args))
    }

    fn read_session_info(
        &self,
        _session_id: &str,
        _worktree_path: &Path,
    ) -> ProviderResult<Option<ProviderSessionInfo>> {
        // Mock providers don't have real session files
        Ok(Some(ProviderSessionInfo {
            description: Some("Mock session".to_string()),
        }))
    }

    fn available_models(&self) -> Vec<&str> {
        self.models.iter().map(|s| s.as_str()).collect()
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn supports_resume(&self) -> bool {
        true // Simulate resume support
    }

    fn has_local_sessions(&self) -> bool {
        false // Mock doesn't store real sessions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_provider_default() {
        let provider = MockProvider::default();
        assert_eq!(provider.name(), "mock");
        assert_eq!(provider.display_name(), "Mock mock");
    }

    #[test]
    fn test_mock_provider_named() {
        let provider = MockProvider::new("test-ai");
        assert_eq!(provider.name(), "test-ai");
        assert_eq!(provider.display_name(), "Mock test-ai");
    }

    #[test]
    fn test_mock_provider_build_command() {
        let provider = MockProvider::default();
        let config = ProviderConfig::default();
        let result = provider.build_command(&config);
        assert!(result.is_ok());
        let (cmd, _args) = result.unwrap();
        assert_eq!(cmd.to_str().unwrap(), "/bin/sh");
    }

    #[test]
    fn test_mock_provider_models() {
        let provider = MockProvider::default();
        let models = provider.available_models();
        assert_eq!(models.len(), 2);
        assert!(models.contains(&"mock-model-1"));
        assert!(models.contains(&"mock-model-2"));
    }

    #[test]
    fn test_mock_provider_custom_models() {
        let provider = MockProvider::with_models(
            "custom",
            vec!["alpha".to_string(), "beta".to_string()],
            "alpha".to_string(),
        );
        assert_eq!(provider.default_model(), "alpha");
        assert_eq!(provider.available_models().len(), 2);
    }
}
