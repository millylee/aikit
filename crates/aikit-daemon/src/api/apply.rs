use super::{mutate_config, ApiError, AppState, ConfigResponse};
use aikit_core::config::ActiveSelection;
use axum::{extract::State, Json};

#[derive(serde::Deserialize)]
pub struct SelectionPayload {
    pub provider_id: String,
    pub api_key_id: String,
    pub model_id: String,
}

pub async fn set_selection(
    State(state): State<AppState>,
    Json(payload): Json<SelectionPayload>,
) -> Result<Json<ConfigResponse>, ApiError> {
    let selection = ActiveSelection {
        provider_id: payload.provider_id,
        api_key_id: payload.api_key_id,
        model_id: payload.model_id,
    };
    {
        let config = super::load_config(&state.config_path)?;
        aikit_core::apply::validate_active_selection(&config, &selection)
            .map_err(|err| ApiError::BadRequest(err.to_string()))?;
    }
    let config = mutate_config(&state, |config| {
        config.active_selection = Some(selection);
        Ok(())
    })?;
    Ok(Json(ConfigResponse::from_config(&config)))
}

#[derive(serde::Deserialize)]
pub struct TargetEnabledPayload {
    pub id: String,
    pub enabled: bool,
    pub config_path: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct TargetsPayload {
    pub targets: Option<Vec<TargetEnabledPayload>>,
    pub claude_pin_models: Option<bool>,
    pub context_1m: Option<bool>,
    pub bypass_permissions: Option<bool>,
    pub max_thinking_effort: Option<bool>,
    pub claude_disable_betas: Option<bool>,
}

pub async fn set_targets(
    State(state): State<AppState>,
    Json(payload): Json<TargetsPayload>,
) -> Result<Json<ConfigResponse>, ApiError> {
    let updates = payload.targets.unwrap_or_default();
    if updates.is_empty()
        && payload.claude_pin_models.is_none()
        && payload.context_1m.is_none()
        && payload.bypass_permissions.is_none()
        && payload.max_thinking_effort.is_none()
        && payload.claude_disable_betas.is_none()
    {
        return Err(ApiError::BadRequest("no target updates provided".into()));
    }
    let config = mutate_config(&state, |config| {
        for update in &updates {
            let target = config
                .targets
                .iter_mut()
                .find(|target| target.id == update.id)
                .ok_or_else(|| {
                    aikit_core::AikitError::Provider(format!("target not found: {}", update.id))
                })?;
            target.enabled = update.enabled;
            if let Some(path) = &update.config_path {
                let trimmed = path.trim();
                target.config_path = if trimmed.is_empty() {
                    None
                } else {
                    Some(std::path::PathBuf::from(trimmed))
                };
            }
        }
        if let Some(value) = payload.claude_pin_models {
            config.claude_pin_models = value;
        }
        if let Some(value) = payload.context_1m {
            config.context_1m = value;
        }
        if let Some(value) = payload.bypass_permissions {
            config.bypass_permissions = value;
        }
        if let Some(value) = payload.max_thinking_effort {
            config.max_thinking_effort = value;
        }
        if let Some(value) = payload.claude_disable_betas {
            config.claude_disable_betas = value;
        }
        Ok(())
    })?;
    Ok(Json(ConfigResponse::from_config(&config)))
}

pub async fn apply_selection(
    State(state): State<AppState>,
) -> Result<Json<aikit_core::apply::ApplyReport>, ApiError> {
    let report = aikit_core::apply::apply_active_selection(&state.config_path)?;
    Ok(Json(report))
}
