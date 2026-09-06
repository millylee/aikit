use std::time::Instant;

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};

const INDEX_HTML: &str = include_str!("../../assets/index.html");

#[derive(Clone)]
pub struct AppState {
    pub token: String,
    pub started_at: Instant,
}

pub fn router(token: &str) -> Router {
    let state = AppState {
        token: token.to_string(),
        started_at: Instant::now(),
    };

    let protected = Router::new()
        .route("/ping", get(ping))
        .layer(middleware::from_fn_with_state(state.clone(), auth));

    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .nest("/api", protected)
        .with_state(state)
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
