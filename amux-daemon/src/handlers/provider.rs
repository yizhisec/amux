//! Provider listing handlers

use crate::state::SharedState;
use amux_proto::daemon::{ListProvidersResponse, ProviderInfo};
use tonic::Status;

/// List all available providers
pub async fn list_providers(
    state: &SharedState,
) -> Result<tonic::Response<ListProvidersResponse>, Status> {
    let state = state.read().await;
    let registry = &state.provider_registry;

    let providers = registry
        .list_providers()
        .iter()
        .map(|name| {
            if let Some(provider) = registry.get(name) {
                let models: Vec<String> = provider
                    .available_models()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                ProviderInfo {
                    name: provider.name().to_string(),
                    display_name: provider.display_name().to_string(),
                    models,
                    default_model: provider.default_model().to_string(),
                }
            } else {
                // Fallback (shouldn't happen)
                ProviderInfo {
                    name: name.to_string(),
                    display_name: name.to_string(),
                    models: vec![],
                    default_model: "".to_string(),
                }
            }
        })
        .collect();

    Ok(tonic::Response::new(ListProvidersResponse { providers }))
}
