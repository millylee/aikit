use aikit_core::config::{
    default_config_path, model_cache_path, state_path, AikitConfig, ApiKeyConfig,
    ImportPromptState, ModelCache, ProviderConfig, TargetConfig,
};
use tempfile::tempdir;

#[test]
fn saves_and_loads_config_as_toml() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("aikit").join("config.toml");
    let config = AikitConfig {
        providers: vec![ProviderConfig {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            enabled: true,
            api_keys: vec![ApiKeyConfig {
                id: "work".into(),
                name: "Work".into(),
                value: "sk-test".into(),
            }],
            manual_models: Vec::new(),
            models_cache: Some(ModelCache {
                refreshed_at: "2026-06-27T00:00:00Z".into(),
                models: vec!["openai/gpt-4.1-mini".into()],
                last_error: None,
            }),
        }],
        active_selection: None,
        import_prompt: ImportPromptState {
            skipped_fingerprint: Some("skip-me".into()),
        },
        targets: vec![TargetConfig {
            id: "codex".into(),
            enabled: true,
            config_path: None,
        }],
        backup_history: vec![],
    };

    config.save_with_sidecars(&path).unwrap();
    let config_content = std::fs::read_to_string(&path).unwrap();
    assert!(!config_content.contains("models_cache"));
    assert!(!config_content.contains("import_prompt"));
    assert!(!config_content.contains("backup_history"));

    assert!(state_path(&path).exists());
    assert!(model_cache_path(&path).exists());
    let loaded = AikitConfig::load_with_sidecars(&path).unwrap();

    assert_eq!(loaded.providers[0].id, "openrouter");
    assert_eq!(loaded.providers[0].api_keys[0].value, "sk-test");
    assert_eq!(
        loaded.providers[0].models_cache.as_ref().unwrap().models,
        vec!["openai/gpt-4.1-mini"]
    );
    assert_eq!(
        loaded.import_prompt.skipped_fingerprint.as_deref(),
        Some("skip-me")
    );
    assert_eq!(loaded.targets[0].id, "codex");
}

#[test]
fn default_path_ends_with_aikit_config_toml() {
    let path = default_config_path().unwrap();
    assert!(path.ends_with(".aikit/config.toml") || path.ends_with(".aikit\\config.toml"));
}
