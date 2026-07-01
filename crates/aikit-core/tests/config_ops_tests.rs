use aikit_core::{
    config::{ActiveSelection, AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig},
    config_ops::{
        add_provider, backup_config_file, delete_api_key, delete_model, delete_provider,
        ProviderForm,
    },
};
use tempfile::tempdir;

#[test]
fn add_provider_validates_unique_id_and_url() {
    let mut config = AikitConfig::default();

    add_provider(
        &mut config,
        ProviderForm {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            enabled: true,
        },
    )
    .unwrap();

    assert_eq!(config.providers.len(), 1);
    assert!(add_provider(
        &mut config,
        ProviderForm {
            id: "openrouter".into(),
            name: "Duplicate".into(),
            base_url: "https://dup.example/v1".into(),
            enabled: true,
        },
    )
    .is_err());
}

#[test]
fn delete_provider_clears_active_selection_and_cache() {
    let mut config = sample_config();

    delete_provider(&mut config, "provider").unwrap();

    assert!(config.providers.is_empty());
    assert!(config.active_selection.is_none());
}

#[test]
fn delete_api_key_clears_active_selection_for_that_key() {
    let mut config = sample_config();

    delete_api_key(&mut config, "provider", "key").unwrap();

    assert!(config.providers[0].api_keys.is_empty());
    assert!(config.active_selection.is_none());
}

#[test]
fn delete_model_removes_manual_model() {
    let mut config = sample_config();
    config.providers[0].manual_models = vec!["manual-a".into(), "manual-b".into()];

    delete_model(&mut config, "provider", "manual-a").unwrap();

    assert_eq!(
        config.providers[0].manual_models,
        vec!["manual-b".to_string()]
    );
}

#[test]
fn delete_model_clears_active_selection_when_model_matches() {
    let mut config = sample_config();
    config.providers[0].manual_models = vec!["manual-a".into()];
    config.active_selection = Some(ActiveSelection {
        provider_id: "provider".into(),
        api_key_id: "key".into(),
        model_id: "manual-a".into(),
    });

    delete_model(&mut config, "provider", "manual-a").unwrap();

    assert!(config.active_selection.is_none());
}

#[test]
fn delete_model_keeps_active_selection_when_other_model_active() {
    let mut config = sample_config();
    config.providers[0].manual_models = vec!["manual-a".into(), "manual-b".into()];
    config.active_selection = Some(ActiveSelection {
        provider_id: "provider".into(),
        api_key_id: "key".into(),
        model_id: "manual-b".into(),
    });

    delete_model(&mut config, "provider", "manual-a").unwrap();

    assert_eq!(
        config.active_selection.as_ref().unwrap().model_id,
        "manual-b"
    );
}

#[test]
fn delete_model_errors_when_model_not_in_manual_models() {
    let mut config = sample_config();
    // "model" only exists in the cache, not in manual_models.
    assert!(delete_model(&mut config, "provider", "model").is_err());
}

#[test]
fn backup_config_file_copies_existing_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "version = 1").unwrap();

    let backup = backup_config_file(&path).unwrap().unwrap();

    assert!(backup.exists());
    assert_eq!(std::fs::read_to_string(backup).unwrap(), "version = 1");
    assert!(dir.path().join("backups").join("aikit").exists());
    assert!(dir.path().join("logs").join("backups.jsonl").exists());
}

fn sample_config() -> AikitConfig {
    AikitConfig {
        providers: vec![ProviderConfig {
            id: "provider".into(),
            name: "Provider".into(),
            base_url: "https://example.com/v1".into(),
            enabled: true,
            api_keys: vec![ApiKeyConfig {
                id: "key".into(),
                name: "Key".into(),
                value: "sk".into(),
            }],
            manual_models: Vec::new(),
            models_cache: Some(ModelCache {
                refreshed_at: "old".into(),
                models: vec!["model".into()],
                last_error: None,
            }),
        }],
        active_selection: Some(ActiveSelection {
            provider_id: "provider".into(),
            api_key_id: "key".into(),
            model_id: "model".into(),
        }),
        ..AikitConfig::default()
    }
}
