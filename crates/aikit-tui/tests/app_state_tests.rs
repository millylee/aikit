use aikit_core::config::{
    ActiveSelection, AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig, TargetConfig,
};
use aikit_tui::{
    app::{active_target_selection, apply_active_selection, AppState, FocusedPane, ModalState},
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
    assert_eq!(state.focused_pane, FocusedPane::Selection);
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
fn m_opens_add_model_modal() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );

    let action = handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE),
    );

    assert_eq!(action, AppAction::None);
    assert!(matches!(state.modal_state, ModalState::ModelForm(_)));
}

#[test]
fn load_config_populates_visible_provider_key_model_and_target_state() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    let codex_path = dir.path().join("codex").join("config.toml");
    sample_config(codex_path)
        .save_with_sidecars(&config_path)
        .unwrap();

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

    state.focused_pane = FocusedPane::Selection;
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
fn active_target_selection_allows_manual_model_without_cache() {
    let mut config = sample_config(std::path::PathBuf::from("codex.toml"));
    config.providers[0].models_cache = None;
    config.providers[0].manual_models = vec!["manual-model".into()];
    config.active_selection = Some(ActiveSelection {
        provider_id: "provider".into(),
        api_key_id: "key".into(),
        model_id: "manual-model".into(),
    });

    let selection = active_target_selection(&config).unwrap();

    assert_eq!(selection.model, "manual-model");
}

#[test]
fn space_toggles_selected_target() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );
    state.focused_pane = FocusedPane::ApplyTo;

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
    );

    assert!(!state.config.targets[0].enabled);
    assert!(state.target_status("codex").unwrap().contains("disabled"));
}

#[test]
fn space_activates_current_item_without_toggling_unfocused_target() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );
    state.focused_pane = FocusedPane::Providers;
    state.config.active_selection = None;

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
    );

    assert!(state.config.targets[0].enabled);
    assert!(state.target_status("codex").is_none());
    assert_eq!(
        state.config.active_selection.as_ref().unwrap().provider_id,
        "provider"
    );
}

#[test]
fn j_and_k_move_selection_in_focused_pane() {
    let mut config = sample_config(std::path::PathBuf::from("codex.toml"));
    config.providers.push(ProviderConfig {
        id: "other".into(),
        name: "Other".into(),
        base_url: "https://other.example/v1".into(),
        enabled: true,
        api_keys: vec![],
        manual_models: Vec::new(),
        models_cache: None,
    });
    let mut state = AppState::from_config(std::path::PathBuf::from("config.toml"), config);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
    );
    assert_eq!(state.selected_provider().unwrap().id, "other");

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
    );
    assert_eq!(state.selected_provider().unwrap().id, "provider");
}

#[test]
fn t_focuses_targets_and_j_k_move_target_selection() {
    let mut config = sample_config(std::path::PathBuf::from("codex.toml"));
    config.targets.push(TargetConfig {
        id: "gemini".into(),
        enabled: true,
        config_path: None,
    });
    let mut state = AppState::from_config(std::path::PathBuf::from("config.toml"), config);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
    );
    assert_eq!(state.focused_pane, FocusedPane::ApplyTo);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
    );
    assert_eq!(state.selected_target().unwrap().id, "gemini");

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
    );
    assert_eq!(state.selected_target().unwrap().id, "codex");
}

#[test]
fn e_on_apply_to_does_not_open_provider_modal() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );
    state.focused_pane = FocusedPane::ApplyTo;

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
    );

    assert_eq!(state.modal_state, ModalState::None);
    assert!(state.status.contains("toggle target"));
}

#[test]
fn selection_empty_key_and_model_rows_open_add_modals() {
    let config = AikitConfig {
        providers: vec![ProviderConfig {
            id: "provider".into(),
            name: "Provider".into(),
            base_url: "https://example.com/v1".into(),
            enabled: true,
            api_keys: vec![],
            manual_models: Vec::new(),
            models_cache: None,
        }],
        ..AikitConfig::default()
    };
    let mut state = AppState::from_config(std::path::PathBuf::from("config.toml"), config);
    state.focused_pane = FocusedPane::Selection;

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert!(matches!(state.modal_state, ModalState::ApiKeyForm(_)));

    state.cancel_modal();
    state.select_next();
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert!(matches!(state.modal_state, ModalState::ModelForm(_)));
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
        manual_models: Vec::new(),
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
    config.save_with_sidecars(&config_path).unwrap();

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

#[test]
fn apply_active_selection_without_api_key_reports_actionable_message() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    let config = AikitConfig {
        providers: vec![ProviderConfig {
            id: "anyrouter".into(),
            name: "Anyrouter".into(),
            base_url: "https://anyrouter.top/v1".into(),
            enabled: true,
            api_keys: vec![],
            manual_models: Vec::new(),
            models_cache: None,
        }],
        targets: vec![TargetConfig {
            id: "claude".into(),
            enabled: true,
            config_path: None,
        }],
        ..AikitConfig::default()
    };
    config.save_with_sidecars(&config_path).unwrap();
    let mut state = AppState::new(config_path);
    state.load_config().unwrap();

    let outcome = state.apply_active_selection().unwrap();

    assert_eq!(outcome.succeeded, 0);
    assert_eq!(outcome.failed, 0);
    assert_eq!(
        outcome.message,
        "Add an API key with + before applying targets"
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
    config.save_with_sidecars(&config_path).unwrap();

    let client = aikit_core::provider::OpenAiCompatibleClient::new(reqwest::Client::new());
    let mut state = AppState::new(config_path.clone());
    state.load_config().unwrap();

    let outcome = state.refresh_active_models(&client).await.unwrap();

    assert_eq!(outcome.succeeded, 1);
    assert_eq!(state.selected_model(), Some("model-new"));
    let saved = AikitConfig::load_with_sidecars(&config_path).unwrap();
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
    state.set_modal_field("name", "OpenRouter").unwrap();
    state
        .set_modal_field("base_url", "https://openrouter.ai/api/v1")
        .unwrap();
    state.save_modal().unwrap();

    assert_eq!(state.config.providers[0].id, "openrouter");
}

#[test]
fn provider_modal_hides_internal_id_and_enabled_fields() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        AikitConfig::default(),
    );

    state.open_add_provider_modal();

    assert!(state.set_modal_field("id", "internal").is_err());
    assert!(state.set_modal_field("enabled", "false").is_err());
}

#[test]
fn provider_modal_model_adds_manual_model_and_selects_when_key_exists() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );

    state.open_edit_provider_modal().unwrap();
    state.set_modal_field("model", "provider-model").unwrap();
    state.save_modal().unwrap();

    assert!(state.config.providers[0]
        .manual_models
        .contains(&"provider-model".to_string()));
    assert_eq!(
        state.config.active_selection.as_ref().unwrap().model_id,
        "provider-model"
    );
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
                manual_models: Vec::new(),
                models_cache: None,
            }],
            ..AikitConfig::default()
        },
    );

    state.open_add_api_key_modal().unwrap();
    state.set_modal_field("value", "sk-test").unwrap();
    state.save_modal().unwrap();

    assert_eq!(state.config.providers[0].api_keys[0].id, "key-1");
    assert_eq!(state.config.providers[0].api_keys[0].name, "Key 1");
    assert_eq!(state.config.providers[0].api_keys[0].value, "sk-test");
}

#[test]
fn modal_input_editing_keys_apply_to_current_field() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        AikitConfig::default(),
    );
    state.open_add_provider_modal();

    for ch in "OpenAI".chars() {
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
        );
    }
    handle_key(&mut state, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
    );
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('U'), KeyModifiers::SHIFT),
    );
    handle_key(&mut state, KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
    );
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
    );

    match state.modal_state {
        ModalState::ProviderForm(form) => {
            assert_eq!(form.name, "");
            assert_eq!(form.cursor, 0);
        }
        other => panic!("expected provider form, got {other:?}"),
    }
}

#[test]
fn api_key_edit_modal_can_rename_key() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );

    state.open_edit_api_key_modal().unwrap();
    state.set_modal_field("name", "Work Key").unwrap();
    state.save_modal().unwrap();

    assert_eq!(state.config.providers[0].api_keys[0].name, "Work Key");
}

#[test]
fn model_modal_save_adds_manual_model_to_selected_provider() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );

    state.open_add_model_modal().unwrap();
    state.set_modal_field("model", "proxy-model").unwrap();
    state.save_modal().unwrap();

    assert_eq!(
        state.config.providers[0].manual_models,
        vec!["proxy-model".to_string()]
    );
    assert_eq!(state.selected_model(), Some("proxy-model"));
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

#[test]
fn provider_modal_save_persists_existing_single_segment_relative_config() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );
    let original_name = state.config.providers[0].name.clone();
    let unique_suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let relative_path = std::path::PathBuf::from(format!("task4-modal-{unique_suffix}.toml"));
    state.config_path = relative_path.clone();
    state.config.save_to(&relative_path).unwrap();

    state.open_edit_provider_modal().unwrap();
    state
        .set_modal_field("name", "Provider Persisted Update")
        .unwrap();
    state.save_modal().unwrap();

    let saved = AikitConfig::load_from(&relative_path).unwrap();
    assert_eq!(saved.providers[0].name, "Provider Persisted Update");
    assert_ne!(saved.providers[0].name, original_name);
    std::fs::remove_file(relative_path).unwrap();
}

#[test]
fn provider_modal_save_failure_keeps_config_unchanged() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::create_dir(&config_path).unwrap();
    let original = sample_config(dir.path().join("codex").join("config.toml"));
    let mut state = AppState::from_config(config_path, original.clone());

    state.open_edit_provider_modal().unwrap();
    state.set_modal_field("name", "Should Not Persist").unwrap();
    let result = state.save_modal();

    assert!(result.is_err());
    assert_eq!(state.config, original);
}

#[test]
fn delete_provider_save_failure_keeps_config_unchanged() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::create_dir(&config_path).unwrap();
    let original = sample_config(dir.path().join("codex").join("config.toml"));
    let mut state = AppState::from_config(config_path, original.clone());

    state.open_delete_provider_confirmation().unwrap();
    let result = state.confirm_modal();

    assert!(result.is_err());
    assert_eq!(state.config, original);
}

#[test]
fn x_on_model_row_does_not_open_api_key_delete_confirmation() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );
    state.focused_pane = FocusedPane::Selection;
    state.select_next();
    let original_key_count = state.config.providers[0].api_keys.len();

    let action = handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
    );

    assert_eq!(action, AppAction::None);
    assert_eq!(state.modal_state, ModalState::None);
    assert_eq!(state.config.providers[0].api_keys.len(), original_key_count);
}

#[test]
fn modal_ignores_ctrl_char_input() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        AikitConfig::default(),
    );
    state.open_add_provider_modal();

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
    );
    state.modal_append_char('b').unwrap();

    let ModalState::ProviderForm(form) = &state.modal_state else {
        panic!("provider modal should stay open");
    };
    assert_eq!(form.name, "b");
}

#[test]
fn manual_import_prompt_skip_does_not_store_fingerprint() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    AikitConfig::default()
        .save_with_sidecars(&config_path)
        .unwrap();
    let mut state = AppState::new(config_path.clone());
    state.load_config().unwrap();

    state.set_import_candidates_for_test(vec![aikit_core::import::ImportCandidate {
        source: aikit_core::import::ImportSource::Env,
        provider_id: "openai".into(),
        provider_name: "OpenAI".into(),
        base_url: Some("https://api.openai.com/v1".into()),
        api_key_name: Some("OPENAI_API_KEY".into()),
        api_key_value: Some("sk-test".into()),
        model: Some("gpt-4.1-mini".into()),
        warnings: vec![],
    }]);
    state.open_import_prompt().unwrap();
    state.skip_import_prompt().unwrap();

    let saved = AikitConfig::load_with_sidecars(&config_path).unwrap();
    assert!(saved.providers.is_empty());
    assert!(saved.import_prompt.skipped_fingerprint.is_none());
}

#[test]
fn startup_import_prompt_skip_stores_fingerprint_without_writing_provider() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    AikitConfig::default()
        .save_with_sidecars(&config_path)
        .unwrap();
    let mut state = AppState::new(config_path.clone());
    state.load_config().unwrap();

    let plan = aikit_core::import::ImportPlan {
        candidates: vec![aikit_core::import::ImportCandidate {
            source: aikit_core::import::ImportSource::Env,
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: Some("https://api.openai.com/v1".into()),
            api_key_name: Some("OPENAI_API_KEY".into()),
            api_key_value: Some("sk-test".into()),
            model: Some("gpt-4.1-mini".into()),
            warnings: vec![],
        }],
        warnings: vec![],
    };
    state.open_startup_import_prompt_from_plan(plan).unwrap();
    state.skip_import_prompt().unwrap();

    let saved = AikitConfig::load_with_sidecars(&config_path).unwrap();
    assert!(saved.providers.is_empty());
    assert!(saved.import_prompt.skipped_fingerprint.is_some());
}

#[test]
fn import_prompt_confirm_writes_provider() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    AikitConfig::default()
        .save_with_sidecars(&config_path)
        .unwrap();
    let mut state = AppState::new(config_path.clone());
    state.load_config().unwrap();

    state.set_import_candidates_for_test(vec![aikit_core::import::ImportCandidate {
        source: aikit_core::import::ImportSource::Env,
        provider_id: "openai".into(),
        provider_name: "OpenAI".into(),
        base_url: Some("https://api.openai.com/v1".into()),
        api_key_name: Some("OPENAI_API_KEY".into()),
        api_key_value: Some("sk-test".into()),
        model: Some("gpt-4.1-mini".into()),
        warnings: vec![],
    }]);
    state.open_import_prompt().unwrap();
    state.confirm_import_all().unwrap();

    let saved = AikitConfig::load_with_sidecars(&config_path).unwrap();
    assert_eq!(saved.providers.len(), 1);
    assert_eq!(saved.providers[0].api_keys[0].value, "sk-test");
}

#[test]
fn import_apply_save_failure_keeps_config_unchanged() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::create_dir(&config_path).unwrap();
    let original = AikitConfig::default();
    let mut state = AppState::from_config(config_path, original.clone());

    state.set_import_candidates_for_test(vec![aikit_core::import::ImportCandidate {
        source: aikit_core::import::ImportSource::Env,
        provider_id: "openai".into(),
        provider_name: "OpenAI".into(),
        base_url: Some("https://api.openai.com/v1".into()),
        api_key_name: Some("OPENAI_API_KEY".into()),
        api_key_value: Some("sk-test".into()),
        model: Some("gpt-4.1-mini".into()),
        warnings: vec![],
    }]);
    state.open_import_prompt().unwrap();
    let result = state.confirm_import_all();

    assert!(result.is_err());
    assert_eq!(state.config, original);
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
            manual_models: Vec::new(),
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
