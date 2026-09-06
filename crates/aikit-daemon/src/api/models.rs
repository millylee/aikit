use super::{load_config, ApiError, AppState};
use aikit_core::provider::OpenAiCompatibleClient;
use axum::{extract::State, Json};

#[derive(serde::Deserialize)]
pub struct RefreshPayload {
    pub provider_id: Option<String>,
    pub api_key_id: Option<String>,
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(payload): Json<RefreshPayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (provider_id, api_key_id) = match (payload.provider_id, payload.api_key_id) {
        (Some(provider_id), Some(api_key_id)) => (provider_id, api_key_id),
        _ => {
            let config = load_config(&state.config_path)?;
            let active = config.active_selection.as_ref().ok_or_else(|| {
                ApiError::BadRequest("no provider/api key given and no active selection".into())
            })?;
            (active.provider_id.clone(), active.api_key_id.clone())
        }
    };

    let client = OpenAiCompatibleClient::new(state.client.clone());
    let count = aikit_core::cache::refresh_selected_models(
        &state.config_path,
        &provider_id,
        &api_key_id,
        &client,
    )
    .await
    .map_err(ApiError::from)?;

    let config = load_config(&state.config_path)?;
    let models = config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| provider.models_cache.clone());
    Ok(Json(serde_json::json!({
        "refreshed": count,
        "models": models.map(|cache| cache.models).unwrap_or_default(),
    })))
}
