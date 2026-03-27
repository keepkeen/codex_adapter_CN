use super::model_info::BASE_INSTRUCTIONS;
use crate::model_provider_info::ModelProviderInfo;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::provider_profiles::bundled_model_catalog;
use codex_protocol::provider_profiles::resolve_bundled_model_lookup_alias;
use std::io;

pub(super) fn bundled_models_for_provider(
    provider_id: Option<&str>,
    provider: &ModelProviderInfo,
) -> io::Result<Vec<ModelInfo>> {
    if let Some(provider_id) = provider_id.or_else(|| provider.known_provider_id())
        && let Some(catalog) = bundled_model_catalog(provider_id)
    {
        return Ok(catalog
            .iter()
            .copied()
            .map(|entry| entry.to_model_info(BASE_INSTRUCTIONS))
            .collect());
    }

    load_openai_catalog()
}

pub(super) fn resolve_model_lookup_alias(
    provider_id: Option<&str>,
    provider: &ModelProviderInfo,
    model: &str,
) -> Option<String> {
    let provider_id = provider_id.or_else(|| provider.known_provider_id())?;
    resolve_bundled_model_lookup_alias(provider_id, model)
}

fn load_openai_catalog() -> io::Result<Vec<ModelInfo>> {
    let file_contents = include_str!("../../models.json");
    let response: codex_protocol::openai_models::ModelsResponse =
        serde_json::from_str(file_contents)?;
    Ok(response.models)
}
