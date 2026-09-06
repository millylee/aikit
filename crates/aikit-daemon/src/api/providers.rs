use super::{load_config, ApiError, AppState, ProviderResponse};
use axum::{
    extract::{Path, State},
    Json,
};

pub async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProviderResponse>>, ApiError> {
    let config = load_config(&state.config_path)?;
    Ok(Json(
        config
            .providers
            .iter()
            .map(ProviderResponse::from_config)
            .collect(),
    ))
}

pub async fn get_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProviderResponse>, ApiError> {
    let config = load_config(&state.config_path)?;
    config
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(ProviderResponse::from_config)
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("provider not found: {id}")))
}
