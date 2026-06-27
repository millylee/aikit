use aikit_core::config::{
    ActiveSelection, AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig, TargetConfig,
};
use aikit_tui::{
    app::{active_target_selection, apply_active_selection, AppState, FocusedPane},
    input::{handle_key, AppAction},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[test]
fn tab_moves_focus_between_three_panes() {
    let mut state = AppState::default();
    assert_eq!(state.focused_pane, FocusedPane::Providers);

    let action = handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(action, AppAction::None);
    assert_eq!(state.focused_pane, FocusedPane::Details);
}

#[test]
fn ctrl_s_requests_apply() {
    let mut state = AppState::default();
    let action = handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );

    assert_eq!(action, AppAction::ApplySelection);
}

#[test]
fn r_requests_model_refresh() {
    let mut state = AppState::default();
    let action = handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    );

    assert_eq!(action, AppAction::RefreshModels);
}

#[test]
fn load_config_populates_visible_provider_key_model_and_target_state() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    let codex_path = dir.path().join("codex").join("config.toml");
    sample_config(codex_path).save_to(&config_path).unwrap();

    let mut state = AppState::new(config_path);
    state.load_config().unwrap();

    assert_eq!(state.provider_index, 0);
    assert_eq!(state.key_index, 0);
    assert_eq!(state.model_index, 0);
    assert_eq!(state.target_index, 0);
    assert_eq!(state.selected_provider().unwrap().name, "Provider");
    assert_eq!(state.selected_key().unwrap().name, "Key");
    assert_eq!(state.selected_model().unwrap(), "model-active");
    assert_eq!(state.selected_target().unwrap().id, "codex");
}

#[test]
fn enter_selects_active_key_and_model_independently() {
    let mut config = sample_config(std::path::PathBuf::from("codex.toml"));
    config.providers[0].api_keys.push(ApiKeyConfig {
        id: "backup".into(),
        name: "Backup".into(),
        value: "sk-backup".into(),
    });
    let mut state = AppState::from_config(std::path::PathBuf::from("config.toml"), config);

    state.focused_pane = FocusedPane::Details;
    state.select_next();
    state.select_next();
    state.activate_selected();

    assert_eq!(
        state.config.active_selection.as_ref().unwrap().api_key_id,
        "backup"
    );
    assert_eq!(
        state.config.active_selection.as_ref().unwrap().model_id,
        "model-active"
    );

    state.select_next();
    state.activate_selected();

    assert_eq!(
        state.config.active_selection.as_ref().unwrap().api_key_id,
        "backup"
    );
    assert_eq!(
        state.config.active_selection.as_ref().unwrap().model_id,
        "model-other"
    );
}

#[test]
fn space_toggles_selected_target() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );
    state.focused_pane = FocusedPane::Targets;

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
    );

    assert!(!state.config.targets[0].enabled);
    assert!(state.target_status("codex").unwrap().contains("disabled"));
}

#[test]
fn arrow_keys_move_selection_in_focused_pane() {
    let mut config = sample_config(std::path::PathBuf::from("codex.toml"));
    config.providers.push(ProviderConfig {
        id: "other".into(),
        name: "Other".into(),
        base_url: "https://other.example/v1".into(),
        enabled: true,
        api_keys: vec![],
        models_cache: None,
    });
    let mut state = AppState::from_config(std::path::PathBuf::from("config.toml"), config);

    handle_key(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

    assert_eq!(state.selected_provider().unwrap().id, "other");
}

#[test]
fn active_target_selection_uses_configured_provider_key_and_cached_model() {
    let config = sample_config(std::path::PathBuf::from("codex.toml"));

    let selection = active_target_selection(&config).unwrap();

    assert_eq!(selection.base_url, "https://example.com/v1");
    assert_eq!(selection.api_key, "sk-active");
    assert_eq!(selection.model, "model-active");
}

#[test]
fn apply_active_selection_writes_enabled_targets_and_skips_disabled_targets() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    let codex_path = dir.path().join("codex").join("config.toml");
    let gemini_path = dir.path().join("gemini").join("settings.json");
    let mut config = sample_config(codex_path.clone());
    config.targets.push(TargetConfig {
        id: "gemini".into(),
        enabled: false,
        config_path: Some(gemini_path.clone()),
    });
    config.save_to(&config_path).unwrap();

    let outcome = apply_active_selection(&config_path).unwrap();

    assert_eq!(outcome.succeeded, 1);
    assert_eq!(outcome.failed, 0);
    assert!(codex_path.exists());
    assert!(!gemini_path.exists());

    let codex: toml::Value = toml::from_str(&std::fs::read_to_string(codex_path).unwrap()).unwrap();
    assert_eq!(
        codex.get("model").and_then(|value| value.as_str()),
        Some("model-active")
    );
}

#[tokio::test]
async fn refresh_models_uses_selected_provider_and_key_before_model_is_active() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{ "id": "model-new" }]
        })))
        .mount(&server)
        .await;

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    let mut config = sample_config(dir.path().join("codex").join("config.toml"));
    config.providers[0].base_url = format!("{}/v1", server.uri());
    config.providers[0].models_cache = None;
    config.active_selection = None;
    config.save_to(&config_path).unwrap();

    let client = aikit_core::provider::OpenAiCompatibleClient::new(reqwest::Client::new());
    let mut state = AppState::new(config_path.clone());
    state.load_config().unwrap();

    let outcome = state.refresh_active_models(&client).await.unwrap();

    assert_eq!(outcome.succeeded, 1);
    assert_eq!(state.selected_model(), Some("model-new"));
    let saved = AikitConfig::load_from(&config_path).unwrap();
    assert_eq!(
        saved.providers[0]
            .models_cache
            .as_ref()
            .unwrap()
            .models
            .as_slice(),
        ["model-new"]
    );
}

#[test]
fn provider_modal_save_adds_provider() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        AikitConfig::default(),
    );

    state.open_add_provider_modal();
    state.set_modal_field("id", "openrouter").unwrap();
    state.set_modal_field("name", "OpenRouter").unwrap();
    state
        .set_modal_field("base_url", "https://openrouter.ai/api/v1")
        .unwrap();
    state.save_modal().unwrap();

    assert_eq!(state.config.providers[0].id, "openrouter");
}

#[test]
fn api_key_modal_save_adds_key_to_selected_provider() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        AikitConfig {
            providers: vec![ProviderConfig {
                id: "provider".into(),
                name: "Provider".into(),
                base_url: "https://example.com/v1".into(),
                enabled: true,
                api_keys: vec![],
                models_cache: None,
            }],
            ..AikitConfig::default()
        },
    );

    state.open_add_api_key_modal().unwrap();
    state.set_modal_field("id", "default").unwrap();
    state.set_modal_field("name", "Default").unwrap();
    state.set_modal_field("value", "sk-test").unwrap();
    state.save_modal().unwrap();

    assert_eq!(state.config.providers[0].api_keys[0].id, "default");
}

#[test]
fn delete_provider_confirmation_clears_provider() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );

    state.open_delete_provider_confirmation().unwrap();
    state.confirm_modal().unwrap();

    assert!(state.config.providers.is_empty());
}

fn sample_config(codex_path: std::path::PathBuf) -> AikitConfig {
    AikitConfig {
        providers: vec![ProviderConfig {
            id: "provider".into(),
            name: "Provider".into(),
            base_url: "https://example.com/v1".into(),
            enabled: true,
            api_keys: vec![ApiKeyConfig {
                id: "key".into(),
                name: "Key".into(),
                value: "sk-active".into(),
            }],
            models_cache: Some(ModelCache {
                refreshed_at: "2026-06-27T00:00:00Z".into(),
                models: vec!["model-active".into(), "model-other".into()],
                last_error: None,
            }),
        }],
        active_selection: Some(ActiveSelection {
            provider_id: "provider".into(),
            api_key_id: "key".into(),
            model_id: "model-active".into(),
        }),
        import_prompt: Default::default(),
        targets: vec![TargetConfig {
            id: "codex".into(),
            enabled: true,
            config_path: Some(codex_path),
        }],
        backup_history: Vec::new(),
    }
}
