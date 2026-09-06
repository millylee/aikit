mod apply;
mod config;
mod import;
mod models;
mod providers;
mod updates;

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use aikit_core::config::{
    ActiveSelection, AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig, TargetConfig,
};
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};

const INDEX_HTML: &str = include_str!("../../assets/index.html");

#[derive(Clone)]
pub struct AppState {
    pub token: String,
    pub started_at: Instant,
    pub config_path: PathBuf,
    pub client: reqwest::Client,
}

pub fn router(token: &str, config_path: PathBuf) -> Router {
    let state = AppState {
        token: token.to_string(),
        started_at: Instant::now(),
        config_path,
        client: reqwest::Client::new(),
    };

    let protected = Router::new()
        .route("/ping", get(ping))
        .route("/config", get(config::get_config))
        .route(
            "/providers",
            get(providers::list_providers).post(providers::create_provider),
        )
        .route(
            "/providers/{id}",
            get(providers::get_provider)
                .put(providers::update_provider)
                .delete(providers::delete_provider),
        )
        .route("/providers/{id}/keys", post(providers::create_api_key))
        .route(
            "/providers/{id}/keys/{key_id}",
            put(providers::update_api_key).delete(providers::delete_api_key),
        )
        .route("/selection", put(apply::set_selection))
        .route("/targets", put(apply::set_targets))
        .route("/apply", post(apply::apply_selection))
        .route("/import/scan", post(import::scan))
        .route("/import/apply", post(import::apply))
        .route("/models/refresh", post(models::refresh))
        .route("/updates/check", post(updates::check))
        .layer(middleware::from_fn_with_state(state.clone(), auth));

    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .nest("/api", protected)
        .with_state(state)
}

pub fn load_config(path: &Path) -> Result<AikitConfig, aikit_core::AikitError> {
    aikit_core::apply::load_or_default(path)
}

pub fn mutate_config<F>(
    state: &AppState,
    mutation: F,
) -> Result<AikitConfig, aikit_core::AikitError>
where
    F: FnOnce(&mut AikitConfig) -> Result<(), aikit_core::AikitError>,
{
    let mut config = load_config(&state.config_path)?;
    mutation(&mut config)?;
    config.save_with_sidecars(&state.config_path)?;
    Ok(config)
}

pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Internal(aikit_core::AikitError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(message) => (StatusCode::NOT_FOUND, message),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Internal(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        let body = Json(serde_json::json!({ "error": message }));
        (status, body).into_response()
    }
}

impl From<aikit_core::AikitError> for ApiError {
    fn from(err: aikit_core::AikitError) -> Self {
        ApiError::Internal(err)
    }
}

#[derive(serde::Deserialize)]
pub struct ProviderPayload {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
}

#[derive(serde::Deserialize)]
pub struct ApiKeyPayload {
    pub id: Option<String>,
    pub name: String,
    pub value: String,
}

#[derive(serde::Serialize)]
pub struct ConfigResponse {
    pub providers: Vec<ProviderResponse>,
    pub active_selection: Option<ActiveSelection>,
    pub targets: Vec<TargetConfig>,
    pub claude_pin_models: bool,
    pub claude_1m_context: bool,
    pub bypass_permissions: bool,
}

impl ConfigResponse {
    pub fn from_config(config: &AikitConfig) -> Self {
        Self {
            providers: config
                .providers
                .iter()
                .map(ProviderResponse::from_config)
                .collect(),
            active_selection: config.active_selection.clone(),
            targets: config.targets.clone(),
            claude_pin_models: config.claude_pin_models,
            claude_1m_context: config.claude_1m_context,
            bypass_permissions: config.bypass_permissions,
        }
    }
}

#[derive(serde::Serialize)]
pub struct ProviderResponse {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub api_keys: Vec<ApiKeyResponse>,
    pub manual_models: Vec<String>,
    pub models_cache: Option<ModelCache>,
}

impl ProviderResponse {
    pub fn from_config(provider: &ProviderConfig) -> Self {
        Self {
            id: provider.id.clone(),
            name: provider.name.clone(),
            base_url: provider.base_url.clone(),
            enabled: provider.enabled,
            api_keys: provider
                .api_keys
                .iter()
                .map(ApiKeyResponse::from_config)
                .collect(),
            manual_models: provider.manual_models.clone(),
            models_cache: provider.models_cache.clone(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub name: String,
    pub value_masked: String,
}

impl ApiKeyResponse {
    pub fn from_config(key: &ApiKeyConfig) -> Self {
        Self {
            id: key.id.clone(),
            name: key.name.clone(),
            value_masked: aikit_core::mask_secret(&key.value),
        }
    }
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "aikit",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.started_at.elapsed().as_secs(),
    }))
}

async fn ping() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

async fn auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let expected = format!("Bearer {}", state.token);
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);
    if authorized {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
