use super::{load_config, ApiError, AppState, ConfigResponse};
use axum::{extract::State, Json};

pub async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigResponse>, ApiError> {
    let config = load_config(&state.config_path)?;
    Ok(Json(ConfigResponse::from_config(&config)))
}
