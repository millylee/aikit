use aikit_core::config::{
    ActiveSelection, AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig, TargetConfig,
};
use aikit_tui::{
    app::{active_target_selection, apply_active_selection, AppState, FocusedPane},
    input::{handle_key, AppAction},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;

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
        targets: vec![TargetConfig {
            id: "codex".into(),
            enabled: true,
            config_path: Some(codex_path),
        }],
        backup_history: Vec::new(),
    }
}
