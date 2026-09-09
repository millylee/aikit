use super::{
    load_config, mutate_config, ApiError, ApiKeyPayload, ApiKeyResponse, AppState, ModelPayload,
    ProviderPayload, ProviderResponse,
};
use aikit_core::config_ops::{ApiKeyForm, ProviderForm};
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

pub async fn create_provider(
    State(state): State<AppState>,
    Json(payload): Json<ProviderPayload>,
) -> Result<(StatusCodeJson, Json<ProviderResponse>), ApiError> {
    let form = provider_form(payload)?;
    let provider_id = form.id.clone();
    mutate_config(&state, |config| {
        aikit_core::config_ops::add_provider(config, form)
    })?;

    let provider = provider_by_id(&state, &provider_id).await?;
    Ok((StatusCodeJson::CREATED, Json(provider)))
}

pub async fn update_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<ProviderPayload>,
) -> Result<Json<ProviderResponse>, ApiError> {
    let new_id = payload
        .id
        .clone()
        .unwrap_or_else(|| id.clone())
        .trim()
        .to_string();
    if new_id.is_empty() {
        return Err(ApiError::BadRequest("provider id cannot be empty".into()));
    }
    let form = ProviderForm {
        id: new_id.clone(),
        name: payload.name,
        base_url: payload.base_url,
        enabled: payload.enabled,
    };
    mutate_config(&state, |config| {
        aikit_core::config_ops::update_provider(config, &id, form)
    })?;

    Ok(Json(provider_by_id(&state, &new_id).await?))
}

pub async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    mutate_config(&state, |config| {
        aikit_core::config_ops::delete_provider(config, &id)
    })?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn create_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<ApiKeyPayload>,
) -> Result<(StatusCodeJson, Json<ApiKeyResponse>), ApiError> {
    let fallback_id = format!("key-{}", next_key_suffix(&state, &id).await);
    let key_id = payload.id.unwrap_or(fallback_id).trim().to_string();
    if key_id.is_empty() {
        return Err(ApiError::BadRequest("api key id cannot be empty".into()));
    }
    let form = ApiKeyForm {
        id: key_id.clone(),
        name: payload.name,
        value: payload.value,
    };
    mutate_config(&state, |config| {
        aikit_core::config_ops::add_api_key(config, &id, form)
    })?;

    let key = key_by_id(&state, &id, &key_id).await?;
    Ok((StatusCodeJson::CREATED, Json(key)))
}

pub async fn update_api_key(
    State(state): State<AppState>,
    Path((id, key_id)): Path<(String, String)>,
    Json(payload): Json<ApiKeyPayload>,
) -> Result<Json<ApiKeyResponse>, ApiError> {
    let new_key_id = payload
        .id
        .clone()
        .unwrap_or_else(|| key_id.clone())
        .trim()
        .to_string();
    if new_key_id.is_empty() {
        return Err(ApiError::BadRequest("api key id cannot be empty".into()));
    }
    let form = ApiKeyForm {
        id: new_key_id.clone(),
        name: payload.name,
        value: payload.value,
    };
    mutate_config(&state, |config| {
        aikit_core::config_ops::update_api_key(config, &id, &key_id, form)
    })?;

    Ok(Json(key_by_id(&state, &id, &new_key_id).await?))
}

pub async fn delete_api_key(
    State(state): State<AppState>,
    Path((id, key_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    mutate_config(&state, |config| {
        aikit_core::config_ops::delete_api_key(config, &id, &key_id)
    })?;
    Ok(Json(serde_json::json!({ "deleted": key_id })))
}

pub async fn create_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<ModelPayload>,
) -> Result<(StatusCodeJson, Json<serde_json::Value>), ApiError> {
    if payload.model.trim().is_empty() {
        return Err(ApiError::BadRequest("model id cannot be empty".into()));
    }
    mutate_config(&state, |config| {
        aikit_core::config_ops::add_model(config, &id, &payload.model)
    })?;
    let manual_models = manual_models_by_provider(&state, &id)?;
    Ok((
        StatusCodeJson::CREATED,
        Json(serde_json::json!({ "manual_models": manual_models })),
    ))
}

pub async fn delete_model(
    State(state): State<AppState>,
    Path((id, model_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !manual_models_by_provider(&state, &id)?.contains(&model_id) {
        return Err(ApiError::NotFound(format!("model not found: {model_id}")));
    }
    mutate_config(&state, |config| {
        aikit_core::config_ops::delete_model(config, &id, &model_id)
    })?;
    Ok(Json(serde_json::json!({ "deleted": model_id })))
}

fn manual_models_by_provider(state: &AppState, id: &str) -> Result<Vec<String>, ApiError> {
    let config = load_config(&state.config_path)?;
    config
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(|provider| provider.manual_models.clone())
        .ok_or_else(|| ApiError::NotFound(format!("provider not found: {id}")))
}

fn provider_form(payload: ProviderPayload) -> Result<ProviderForm, ApiError> {
    let id = payload.id.unwrap_or_default().trim().to_string();
    if id.is_empty() {
        return Err(ApiError::BadRequest("provider id cannot be empty".into()));
    }
    Ok(ProviderForm {
        id,
        name: payload.name,
        base_url: payload.base_url,
        enabled: payload.enabled,
    })
}

async fn provider_by_id(state: &AppState, id: &str) -> Result<ProviderResponse, ApiError> {
    let config = load_config(&state.config_path)?;
    config
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(ProviderResponse::from_config)
        .ok_or_else(|| ApiError::NotFound(format!("provider not found: {id}")))
}

async fn key_by_id(state: &AppState, id: &str, key_id: &str) -> Result<ApiKeyResponse, ApiError> {
    let config = load_config(&state.config_path)?;
    config
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .and_then(|provider| provider.api_keys.iter().find(|key| key.id == key_id))
        .map(ApiKeyResponse::from_config)
        .ok_or_else(|| ApiError::NotFound(format!("api key not found: {key_id}")))
}

async fn next_key_suffix(state: &AppState, id: &str) -> usize {
    let config = load_config(&state.config_path).unwrap_or_default();
    config
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(|provider| provider.api_keys.len() + 1)
        .unwrap_or(1)
}

type StatusCodeJson = axum::http::StatusCode;
