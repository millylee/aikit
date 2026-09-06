use aikit_core::config::{ActiveSelection, AikitConfig, ApiKeyConfig, ProviderConfig};
use aikit_daemon::api::router;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn app() -> axum::Router {
    let dir = tempfile::tempdir().unwrap();
    router("test-token", dir.path().join("config.toml"))
}

fn authorized_get(path: &str) -> Request<Body> {
    Request::get(path)
        .header("Authorization", "Bearer test-token")
        .body(Body::empty())
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn seeded_router() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let mut config = AikitConfig::default();
    config.providers.push(ProviderConfig {
        id: "p1".into(),
        name: "Provider One".into(),
        base_url: "https://example.com/v1".into(),
        enabled: true,
        api_keys: vec![
            ApiKeyConfig {
                id: "k1".into(),
                name: "long key".into(),
                value: "sk-1234567890abcdef".into(),
            },
            ApiKeyConfig {
                id: "k2".into(),
                name: "short key".into(),
                value: "short".into(),
            },
        ],
        manual_models: vec!["manual-model".into()],
        models_cache: None,
    });
    config.active_selection = Some(ActiveSelection {
        provider_id: "p1".into(),
        api_key_id: "k1".into(),
        model_id: "m1".into(),
    });
    config.save_with_sidecars(&config_path).unwrap();
    (router("test-token", config_path), dir)
}

#[tokio::test]
async fn health_is_public_and_reports_version() {
    let app = app();
    let response = app
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["name"], "aikit");
    assert!(!json["version"].as_str().unwrap_or_default().is_empty());
}

#[tokio::test]
async fn index_serves_embedded_page() {
    let app = app();
    let response = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("aikit"));
}

#[tokio::test]
async fn protected_endpoints_reject_missing_token() {
    let app = app();
    for path in ["/api/ping", "/api/config", "/api/providers"] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn protected_endpoints_reject_wrong_token() {
    let app = app();
    let response = app
        .oneshot(
            Request::get("/api/ping")
                .header("Authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_endpoints_accept_valid_token() {
    let app = app();
    let response = app.oneshot(authorized_get("/api/ping")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn config_defaults_when_file_missing() {
    let app = app();
    let response = app.oneshot(authorized_get("/api/config")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["providers"].as_array().unwrap().len(), 0);
    assert_eq!(json["claude_pin_models"], true);
    assert_eq!(json["bypass_permissions"], false);
}

#[tokio::test]
async fn config_masks_api_key_values() {
    let (app, _dir) = seeded_router();
    let response = app
        .clone()
        .oneshot(authorized_get("/api/config"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let raw = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!raw.contains("sk-1234567890abcdef"), "plaintext key leaked");

    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let keys = json["providers"][0]["api_keys"].as_array().unwrap();
    assert_eq!(keys[0]["value_masked"], "sk-1...cdef");
    assert_eq!(keys[1]["value_masked"], "***");
    assert_eq!(json["active_selection"]["provider_id"], "p1");
    assert_eq!(json["providers"][0]["manual_models"][0], "manual-model");
}

#[tokio::test]
async fn providers_list_and_get_by_id() {
    let (app, _dir) = seeded_router();

    let response = app
        .clone()
        .oneshot(authorized_get("/api/providers"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let providers = json.as_array().unwrap();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0]["id"], "p1");
    assert_eq!(providers[0]["api_keys"][0]["value_masked"], "sk-1...cdef");

    let response = app
        .clone()
        .oneshot(authorized_get("/api/providers/p1"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["base_url"], "https://example.com/v1");

    let response = app
        .oneshot(authorized_get("/api/providers/missing"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
