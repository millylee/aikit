use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
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
