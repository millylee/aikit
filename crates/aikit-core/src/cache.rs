use std::path::Path;

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    apply::load_or_default,
    config::{ModelCache, ProviderConfig},
    provider::OpenAiCompatibleClient,
    AikitError, Result,
};

pub async fn refresh_models(
    provider: &mut ProviderConfig,
    api_key_id: &str,
    client: &OpenAiCompatibleClient,
) -> Result<()> {
    let key = provider
        .api_keys
        .iter()
        .find(|key| key.id == api_key_id)
        .ok_or_else(|| AikitError::Provider(format!("api key not found: {api_key_id}")))?;

    match client.list_models(&provider.base_url, &key.value).await {
        Ok(models) => {
            provider.models_cache = Some(ModelCache {
                refreshed_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
                models,
                last_error: None,
            });
            Ok(())
        }
        Err(err) => {
            if let Some(cache) = provider.models_cache.as_mut() {
                cache.last_error = Some(err.to_string());
            } else {
                provider.models_cache = Some(ModelCache {
                    refreshed_at: String::new(),
                    models: Vec::new(),
                    last_error: Some(err.to_string()),
                });
            }
            Err(err)
        }
    }
}

pub async fn refresh_selected_models(
    config_path: &Path,
    provider_id: &str,
    api_key_id: &str,
    client: &OpenAiCompatibleClient,
) -> Result<usize> {
    let mut config = load_or_default(config_path)?;
    let provider = config
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| {
            AikitError::ConfigParse(format!("selected provider not found: {provider_id}"))
        })?;

    let result = refresh_models(provider, api_key_id, client).await;
    let count = provider
        .models_cache
        .as_ref()
        .map(|cache| cache.models.len())
        .unwrap_or(0);
    config.save_with_sidecars(config_path)?;

    result.map(|_| count)
}
