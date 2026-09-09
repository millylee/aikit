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
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(content_type.starts_with("text/html"));
    assert!(html.contains("aikit"));
    assert!(html.contains("/app.js"));
}

#[tokio::test]
async fn static_assets_serve_with_content_types() {
    let app = app();
    for (path, expected) in [("/app.js", "javascript"), ("/style.css", "text/css")] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.contains(expected), "{path}: {content_type}");
        let cache_control = response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(cache_control, "no-store", "{path} must not be cacheable");
    }
    let response = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cache_control = response
        .headers()
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert_eq!(cache_control, "no-store", "index must not be cacheable");
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
    assert_eq!(json["max_thinking_effort"], false);
    assert_eq!(json["claude_disable_betas"], false);
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

fn post_json(path: &str, body: serde_json::Value) -> Request<Body> {
    request_json("POST", path, body)
}

fn put_json(path: &str, body: serde_json::Value) -> Request<Body> {
    request_json("PUT", path, body)
}

fn delete_json(path: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(path)
        .header("Authorization", "Bearer test-token")
        .body(Body::empty())
        .unwrap()
}

fn request_json(method: &str, path: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("Authorization", "Bearer test-token")
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn provider_crud_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let app = router("test-token", dir.path().join("config.toml"));

    let response = app
        .clone()
        .oneshot(post_json(
            "/api/providers",
            serde_json::json!({
                "id": "p2",
                "name": "New Provider",
                "base_url": "https://new.example/v1",
                "enabled": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    assert_eq!(json["id"], "p2");

    let response = app
        .clone()
        .oneshot(put_json(
            "/api/providers/p2",
            serde_json::json!({
                "name": "Renamed",
                "base_url": "https://new.example/v2",
                "enabled": false
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["name"], "Renamed");
    assert_eq!(json["enabled"], false);

    let response = app
        .clone()
        .oneshot(delete_json("/api/providers/p2"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(authorized_get("/api/providers/p2"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_key_crud_persists_plaintext_and_masks_response() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let app = router("test-token", config_path.clone());
    app.clone()
        .oneshot(post_json(
            "/api/providers",
            serde_json::json!({
                "id": "p1",
                "name": "P1",
                "base_url": "https://example.com/v1",
                "enabled": true
            }),
        ))
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(post_json(
            "/api/providers/p1/keys",
            serde_json::json!({
                "name": "primary",
                "value": "sk-1234567890abcdef"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    assert_eq!(json["value_masked"], "sk-1...cdef");
    let key_id = json["id"].as_str().unwrap().to_string();

    let stored = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        stored.contains("sk-1234567890abcdef"),
        "plaintext not persisted"
    );

    let response = app
        .clone()
        .oneshot(put_json(
            &format!("/api/providers/p1/keys/{key_id}"),
            serde_json::json!({
                "name": "rotated",
                "value": "sk-abcdef1234567890"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["name"], "rotated");

    let response = app
        .oneshot(delete_json(&format!("/api/providers/p1/keys/{key_id}")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn selection_validation_returns_bad_request() {
    let dir = tempfile::tempdir().unwrap();
    let app = router("test-token", dir.path().join("config.toml"));

    let response = app
        .oneshot(put_json(
            "/api/selection",
            serde_json::json!({
                "provider_id": "missing",
                "api_key_id": "k",
                "model_id": "m"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn targets_update_toggles_flags() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let app = router("test-token", config_path.clone());

    for enabled in [true, false, true, false] {
        let response = app
            .clone()
            .oneshot(put_json(
                "/api/targets",
                serde_json::json!({
                    "targets": [{ "id": "claude", "enabled": enabled }],
                    "claude_pin_models": enabled,
                    "context_1m": enabled,
                    "bypass_permissions": enabled,
                    "max_thinking_effort": enabled,
                    "claude_disable_betas": enabled
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        let claude = json["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|target| target["id"] == "claude")
            .unwrap();
        assert_eq!(claude["enabled"], enabled);
        assert_eq!(json["claude_pin_models"], enabled);
        assert_eq!(json["context_1m"], enabled);
        assert_eq!(json["bypass_permissions"], enabled);
        assert_eq!(json["max_thinking_effort"], enabled);
        assert_eq!(json["claude_disable_betas"], enabled);

        let saved = AikitConfig::load_from(&config_path).unwrap();
        assert_eq!(saved.claude_pin_models, enabled);
        assert_eq!(saved.context_1m, enabled);
        assert_eq!(saved.bypass_permissions, enabled);
        assert_eq!(saved.max_thinking_effort, enabled);
        assert_eq!(saved.claude_disable_betas, enabled);
    }
}

#[tokio::test]
async fn apply_writes_enabled_targets() {
    let dir = tempfile::tempdir().unwrap();
    let codex_dir = dir.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let app = router("test-token", dir.path().join("config.toml"));

    app.clone()
        .oneshot(post_json(
            "/api/providers",
            serde_json::json!({
                "id": "p1",
                "name": "P1",
                "base_url": "https://example.com/v1",
                "enabled": true
            }),
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(post_json(
            "/api/providers/p1/keys",
            serde_json::json!({ "name": "k", "value": "sk-1234567890abcdef" }),
        ))
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(put_json(
            "/api/selection",
            serde_json::json!({
                "provider_id": "p1",
                "api_key_id": "key-1",
                "model_id": "glm-5.3"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(put_json(
            "/api/targets",
            serde_json::json!({
                "targets": [
                    { "id": "claude", "enabled": false },
                    { "id": "codex", "enabled": true,
                      "config_path": codex_dir.join("config.toml").to_string_lossy() }
                ]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "targets update failed");

    let response = app
        .clone()
        .oneshot(post_json("/api/apply", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["succeeded"], 1);
    assert_eq!(json["failed"], 0);

    let written = std::fs::read_to_string(codex_dir.join("config.toml")).unwrap();
    assert!(written.contains("AIKIT_API_KEY"));
    assert!(written.contains("glm-5.3"));
}

#[tokio::test]
async fn apply_toggles_codex_bypass_permissions() {
    let (app, dir) = seeded_router();
    let codex_dir = dir.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let codex_path = codex_dir.join("config.toml");

    let response = app
        .clone()
        .oneshot(put_json(
            "/api/targets",
            serde_json::json!({
                "targets": [
                    { "id": "claude", "enabled": false },
                    { "id": "codex", "enabled": true, "config_path": codex_path }
                ]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    for enabled in [true, false, true, false] {
        let response = app
            .clone()
            .oneshot(put_json(
                "/api/targets",
                serde_json::json!({ "bypass_permissions": enabled }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["bypass_permissions"], enabled);

        let response = app
            .clone()
            .oneshot(post_json("/api/apply", serde_json::json!({})))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let report = body_json(response).await;
        assert_eq!(report["succeeded"], 1);
        assert_eq!(report["failed"], 0);

        let written = std::fs::read_to_string(&codex_path).unwrap();
        assert_eq!(written.contains("approval_policy = \"never\""), enabled);
        assert_eq!(
            written.contains("sandbox_mode = \"danger-full-access\""),
            enabled
        );
    }
}

#[tokio::test]
async fn apply_toggles_max_thinking_effort_for_both_targets() {
    let (app, dir) = seeded_router();
    let claude_dir = dir.path().join(".claude");
    let codex_dir = dir.path().join(".codex");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::create_dir_all(&codex_dir).unwrap();
    let claude_path = claude_dir.join("settings.json");
    let codex_path = codex_dir.join("config.toml");

    let response = app
        .clone()
        .oneshot(put_json(
            "/api/targets",
            serde_json::json!({
                "targets": [
                    { "id": "claude", "enabled": true, "config_path": claude_path },
                    { "id": "codex", "enabled": true, "config_path": codex_path }
                ]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    for enabled in [true, false] {
        let response = app
            .clone()
            .oneshot(put_json(
                "/api/targets",
                serde_json::json!({ "max_thinking_effort": enabled }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_json(response).await["max_thinking_effort"], enabled);

        let response = app
            .clone()
            .oneshot(post_json("/api/apply", serde_json::json!({})))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let report = body_json(response).await;
        assert_eq!(report["succeeded"], 2);
        assert_eq!(report["failed"], 0);

        let claude_written = std::fs::read_to_string(&claude_path).unwrap();
        let claude_json: serde_json::Value = serde_json::from_str(&claude_written).unwrap();
        assert_eq!(
            claude_json["env"]["CLAUDE_CODE_EFFORT_LEVEL"] == "max",
            enabled
        );
        assert_eq!(
            claude_json["env"]["CLAUDE_CODE_ALWAYS_ENABLE_EFFORT"] == "1",
            enabled
        );

        let codex_written = std::fs::read_to_string(&codex_path).unwrap();
        assert_eq!(
            codex_written.contains("model_reasoning_effort = \"max\""),
            enabled
        );
    }
}

#[tokio::test]
async fn import_apply_adds_provider() {
    let dir = tempfile::tempdir().unwrap();
    let app = router("test-token", dir.path().join("config.toml"));

    let response = app
        .clone()
        .oneshot(post_json(
            "/api/import/apply",
            serde_json::json!({
                "candidates": [{
                    "source": "Codex",
                    "provider_id": "imported",
                    "provider_name": "Imported",
                    "base_url": "https://import.example/v1",
                    "api_key_name": "OPENAI_API_KEY",
                    "api_key_value": "sk-1234567890abcdef",
                    "model": "imported-model",
                    "warnings": []
                }]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["added_providers"], 1);

    let response = app
        .clone()
        .oneshot(authorized_get("/api/providers/imported"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["api_keys"][0]["value_masked"], "sk-1...cdef");
}

#[tokio::test]
async fn write_endpoints_require_auth() {
    let app = app();
    for (method, path) in [
        ("POST", "/api/providers"),
        ("PUT", "/api/selection"),
        ("PUT", "/api/targets"),
        ("POST", "/api/apply"),
        ("POST", "/api/import/scan"),
        ("POST", "/api/models/refresh"),
        ("POST", "/api/updates/check"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}
