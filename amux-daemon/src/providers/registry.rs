//! Provider registry for managing available AI providers

use super::{AiProvider, ClaudeProvider, CodexProvider, ProviderError, ProviderResult};
#[cfg(test)]
use super::{MockProvider, ProviderRef};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of available AI providers
pub struct ProviderRegistry {
    /// Map of provider name -> provider instance
    providers: HashMap<String, Arc<dyn AiProvider>>,
    /// Default provider name
    default_provider: String,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    /// Create a new registry with default providers
    pub fn new() -> Self {
        let mut providers: HashMap<String, Arc<dyn AiProvider>> = HashMap::new();

        // Register Claude provider
        let claude = Arc::new(ClaudeProvider::new());
        providers.insert(claude.name().to_string(), claude);

        // Register Codex provider
        let codex = Arc::new(CodexProvider::new());
        providers.insert(codex.name().to_string(), codex);

        Self {
            providers,
            default_provider: "claude".to_string(),
        }
    }

    /// Register a new provider
    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// Set the default provider
    pub fn set_default(&mut self, name: &str) -> ProviderResult<()> {
        if self.providers.contains_key(name) {
            self.default_provider = name.to_string();
            Ok(())
        } else {
            Err(ProviderError::not_found(name, &self.list_providers()))
        }
    }

    /// Get a provider by name (returns Option)
    pub fn get(&self, name: &str) -> Option<Arc<dyn AiProvider>> {
        self.providers.get(name).cloned()
    }

    /// Get a provider by name or return error with available providers
    pub fn get_or_error(&self, name: &str) -> ProviderResult<Arc<dyn AiProvider>> {
        self.get(name)
            .ok_or_else(|| ProviderError::not_found(name, &self.list_providers()))
    }

    /// Validate a model for a provider
    pub fn validate_model(&self, provider_name: &str, model: &str) -> ProviderResult<()> {
        let provider = self.get_or_error(provider_name)?;
        let available = provider.available_models();
        if available.contains(&model) {
            Ok(())
        } else {
            Err(ProviderError::invalid_model(
                provider_name,
                model,
                &available,
            ))
        }
    }

    /// Get the default provider
    pub fn default_provider(&self) -> Arc<dyn AiProvider> {
        self.providers
            .get(&self.default_provider)
            .cloned()
            .expect("Default provider must exist")
    }

    /// Get the default provider name
    pub fn default_provider_name(&self) -> &str {
        &self.default_provider
    }

    /// List all registered provider names
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a provider is registered
    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    /// Create a test registry with mock providers only
    ///
    /// This is useful for unit tests that don't need real AI CLI tools.
    /// The mock providers simulate AI behavior without external dependencies.
    #[cfg(test)]
    pub fn test_registry() -> Self {
        let mut providers: HashMap<String, Arc<dyn AiProvider>> = HashMap::new();

        // Register mock providers that simulate claude and codex
        let mock_claude = Arc::new(MockProvider::with_models(
            "claude",
            vec![
                "opus".to_string(),
                "sonnet".to_string(),
                "haiku".to_string(),
            ],
            "sonnet".to_string(),
        ));
        providers.insert("claude".to_string(), mock_claude);

        let mock_codex = Arc::new(MockProvider::with_models(
            "codex",
            vec!["o4-mini".to_string(), "gpt-4".to_string()],
            "o4-mini".to_string(),
        ));
        providers.insert("codex".to_string(), mock_codex);

        Self {
            providers,
            default_provider: "claude".to_string(),
        }
    }

    /// Create a registry for tests, using mocks unless AMUX_E2E_PROVIDERS is set
    ///
    /// When AMUX_E2E_PROVIDERS=1 is set, real providers are used for E2E testing.
    /// Otherwise, mock providers are used for faster, isolated unit tests.
    #[cfg(test)]
    pub fn for_tests() -> Self {
        if std::env::var("AMUX_E2E_PROVIDERS").is_ok() {
            Self::new()
        } else {
            Self::test_registry()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_registry() {
        let registry = ProviderRegistry::new();
        assert!(registry.has_provider("claude"));
        assert!(registry.has_provider("codex"));
        assert_eq!(registry.default_provider_name(), "claude");
    }

    #[test]
    fn test_get_provider() {
        let registry = ProviderRegistry::new();
        let claude = registry.get("claude");
        assert!(claude.is_some());
        assert_eq!(claude.unwrap().name(), "claude");
    }

    #[test]
    fn test_default_provider() {
        let registry = ProviderRegistry::new();
        let default = registry.default_provider();
        assert_eq!(default.name(), "claude");
    }

    #[test]
    fn test_list_providers() {
        let registry = ProviderRegistry::new();
        let providers = registry.list_providers();
        assert!(providers.contains(&"claude"));
        assert!(providers.contains(&"codex"));
        assert_eq!(providers.len(), 2);
    }

    #[test]
    fn test_get_codex_provider() {
        let registry = ProviderRegistry::new();
        let codex = registry.get("codex");
        assert!(codex.is_some());
        let codex_provider = codex.unwrap();
        assert_eq!(codex_provider.name(), "codex");
        assert_eq!(codex_provider.display_name(), "OpenAI Codex");
        assert!(codex_provider.supports_resume());
        assert!(codex_provider.has_local_sessions());
    }

    #[test]
    fn test_test_registry() {
        let registry = ProviderRegistry::test_registry();
        assert!(registry.has_provider("claude"));
        assert!(registry.has_provider("codex"));

        // Verify mock claude has correct models
        let claude = registry.get("claude").unwrap();
        let models = claude.available_models();
        assert!(models.contains(&"opus"));
        assert!(models.contains(&"sonnet"));
        assert!(models.contains(&"haiku"));
    }

    #[test]
    fn test_validate_model_success() {
        let registry = ProviderRegistry::test_registry();
        assert!(registry.validate_model("claude", "sonnet").is_ok());
        assert!(registry.validate_model("codex", "o4-mini").is_ok());
    }

    #[test]
    fn test_validate_model_failure() {
        let registry = ProviderRegistry::test_registry();
        let result = registry.validate_model("claude", "invalid-model");
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Check error message contains useful info
        let msg = err.to_string();
        assert!(msg.contains("invalid-model"));
        assert!(msg.contains("claude"));
        assert!(msg.contains("Available models"));
    }

    #[test]
    fn test_get_or_error_success() {
        let registry = ProviderRegistry::test_registry();
        assert!(registry.get_or_error("claude").is_ok());
    }

    #[test]
    fn test_get_or_error_failure() {
        let registry = ProviderRegistry::test_registry();
        let result = registry.get_or_error("nonexistent");
        assert!(result.is_err());
        // Use match to avoid needing Debug for Arc<dyn AiProvider>
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(msg.contains("nonexistent"));
            assert!(msg.contains("Available providers"));
        }
    }

    #[test]
    fn test_provider_ref_with_defaults() {
        let registry = ProviderRegistry::test_registry();
        // No provider or model specified - should use defaults
        let provider_ref = ProviderRef::new(&registry, None, None).unwrap();
        assert_eq!(provider_ref.name, "claude");
        assert_eq!(provider_ref.model, "sonnet"); // default model for claude
    }

    #[test]
    fn test_provider_ref_explicit_provider() {
        let registry = ProviderRegistry::test_registry();
        let provider_ref = ProviderRef::new(&registry, Some("codex"), None).unwrap();
        assert_eq!(provider_ref.name, "codex");
        assert_eq!(provider_ref.model, "o4-mini"); // default model for codex
    }

    #[test]
    fn test_provider_ref_explicit_model() {
        let registry = ProviderRegistry::test_registry();
        let provider_ref = ProviderRef::new(&registry, Some("claude"), Some("opus")).unwrap();
        assert_eq!(provider_ref.name, "claude");
        assert_eq!(provider_ref.model, "opus");
    }

    #[test]
    fn test_provider_ref_invalid_provider() {
        let registry = ProviderRegistry::test_registry();
        let result = ProviderRef::new(&registry, Some("nonexistent"), None);
        assert!(result.is_err());
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(msg.contains("nonexistent"));
            assert!(msg.contains("not found"));
        }
    }

    #[test]
    fn test_provider_ref_invalid_model() {
        let registry = ProviderRegistry::test_registry();
        let result = ProviderRef::new(&registry, Some("claude"), Some("invalid-model"));
        assert!(result.is_err());
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(msg.contains("invalid-model"));
            assert!(msg.contains("claude"));
        }
    }

    #[test]
    fn test_provider_ref_shell() {
        let shell_ref = ProviderRef::shell();
        assert_eq!(shell_ref.name, "shell");
        assert_eq!(shell_ref.model, "");
    }
}
