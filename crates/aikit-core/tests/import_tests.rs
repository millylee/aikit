use aikit_core::{
    config::{AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig},
    import::{
        apply_import_candidates, candidate_fingerprint, scan_claude_config, scan_codex_config,
        scan_env, scan_gemini_config, ImportCandidate, ImportSource,
    },
};
use tempfile::tempdir;

#[test]
fn env_scan_imports_openai_key_base_url_and_model() {
    let plan = scan_env([
        ("OPENAI_API_KEY".to_string(), "sk-openai".to_string()),
        (
            "OPENAI_BASE_URL".to_string(),
            "https://api.openai.com/v1".to_string(),
        ),
        ("OPENAI_MODEL".to_string(), "gpt-4.1-mini".to_string()),
    ]);

    assert!(plan.warnings.is_empty());
    assert_eq!(plan.candidates.len(), 1);
    let candidate = &plan.candidates[0];
    assert_eq!(candidate.source, ImportSource::Env);
    assert_eq!(candidate.provider_id, "openai");
    assert_eq!(candidate.provider_name, "OpenAI");
    assert_eq!(
        candidate.base_url.as_deref(),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(candidate.api_key_name.as_deref(), Some("OPENAI_API_KEY"));
    assert_eq!(candidate.api_key_value.as_deref(), Some("sk-openai"));
    assert_eq!(candidate.model.as_deref(), Some("gpt-4.1-mini"));
}

#[test]
fn env_scan_imports_anthropic_model_variable() {
    let plan = scan_env([
        ("ANTHROPIC_API_KEY".to_string(), "sk-ant".to_string()),
        (
            "ANTHROPIC_BASE_URL".to_string(),
            "https://anthropic-proxy.example/v1".to_string(),
        ),
        ("ANTHROPIC_MODEL".to_string(), "claude-sonnet-4".to_string()),
    ]);

    let candidate = &plan.candidates[0];
    assert_eq!(candidate.provider_id, "anthropic");
    assert_eq!(candidate.model.as_deref(), Some("claude-sonnet-4"));
}

#[test]
fn env_scan_candidate_fingerprint_changes_when_secret_changes() {
    let first = scan_env([("OPENAI_API_KEY".to_string(), "sk-one".to_string())]);
    let second = scan_env([("OPENAI_API_KEY".to_string(), "sk-two".to_string())]);

    assert_ne!(
        candidate_fingerprint(&first.candidates),
        candidate_fingerprint(&second.candidates)
    );
}

#[test]
fn codex_scan_reads_aikit_provider_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
model = "model-from-codex"
model_provider = "aikit"

[model_providers.aikit]
name = "aikit"
base_url = "https://proxy.example/v1"
api_key = "sk-codex"
"#,
    )
    .unwrap();

    let plan = scan_codex_config(&path);

    assert!(plan.warnings.is_empty());
    let candidate = &plan.candidates[0];
    assert_eq!(candidate.source, ImportSource::Codex);
    assert_eq!(
        candidate.base_url.as_deref(),
        Some("https://proxy.example/v1")
    );
    assert_eq!(candidate.api_key_value.as_deref(), Some("sk-codex"));
    assert_eq!(candidate.model.as_deref(), Some("model-from-codex"));
}

#[test]
fn claude_scan_reads_top_level_model() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"
{
  "theme": "dark",
  "model": "claude-sonnet-4",
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-claude",
    "ANTHROPIC_BASE_URL": "https://claude-proxy.example/v1"
  }
}
"#,
    )
    .unwrap();

    let plan = scan_claude_config(&path);

    assert!(plan.warnings.is_empty());
    let candidate = &plan.candidates[0];
    assert_eq!(candidate.source, ImportSource::Claude);
    assert_eq!(
        candidate.base_url.as_deref(),
        Some("https://claude-proxy.example/v1")
    );
    assert_eq!(candidate.api_key_value.as_deref(), Some("sk-claude"));
    assert_eq!(candidate.model.as_deref(), Some("claude-sonnet-4"));
}

#[test]
fn claude_scan_reads_native_env_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"
{
  "theme": "dark",
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-claude",
    "ANTHROPIC_BASE_URL": "https://claude-proxy.example/v1",
    "ANTHROPIC_MODEL": "claude-sonnet-4"
  }
}
"#,
    )
    .unwrap();

    let plan = scan_claude_config(&path);

    assert!(plan.warnings.is_empty());
    let candidate = &plan.candidates[0];
    assert_eq!(candidate.source, ImportSource::Claude);
    assert_eq!(
        candidate.base_url.as_deref(),
        Some("https://claude-proxy.example/v1")
    );
    assert_eq!(candidate.api_key_value.as_deref(), Some("sk-claude"));
    assert_eq!(candidate.model.as_deref(), Some("claude-sonnet-4"));
}

#[test]
fn invalid_gemini_config_returns_warning_not_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{invalid").unwrap();

    let plan = scan_gemini_config(&path);

    assert!(plan.candidates.is_empty());
    assert_eq!(plan.warnings.len(), 1);
}

#[test]
fn merge_preserves_existing_name_enabled_and_model_cache() {
    let mut config = AikitConfig {
        providers: vec![ProviderConfig {
            id: "existing".into(),
            name: "Custom Name".into(),
            base_url: "https://proxy.example/v1".into(),
            enabled: false,
            api_keys: vec![ApiKeyConfig {
                id: "default".into(),
                name: "Default".into(),
                value: "sk-existing".into(),
            }],
            manual_models: Vec::new(),
            models_cache: Some(ModelCache {
                refreshed_at: "old".into(),
                models: vec!["cached-model".into()],
                last_error: None,
            }),
        }],
        ..AikitConfig::default()
    };

    let result = apply_import_candidates(
        &mut config,
        &[ImportCandidate {
            source: ImportSource::Env,
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: Some("https://proxy.example/v1".into()),
            api_key_name: Some("Imported".into()),
            api_key_value: Some("sk-imported".into()),
            model: Some("imported-model".into()),
            warnings: vec![],
        }],
    );

    assert_eq!(result.updated_providers, 1);
    assert_eq!(config.providers.len(), 1);
    assert_eq!(config.providers[0].name, "Custom Name");
    assert!(!config.providers[0].enabled);
    assert_eq!(
        config.providers[0].models_cache.as_ref().unwrap().models,
        vec!["cached-model"]
    );
    assert_eq!(config.providers[0].manual_models, vec!["imported-model"]);
    assert_eq!(config.providers[0].api_keys.len(), 2);
}

#[test]
fn merge_skips_new_provider_without_base_url() {
    let mut config = AikitConfig::default();

    let result = apply_import_candidates(
        &mut config,
        &[ImportCandidate {
            source: ImportSource::Env,
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: None,
            api_key_name: Some("OPENAI_API_KEY".into()),
            api_key_value: Some("sk-imported".into()),
            model: Some("gpt-4.1-mini".into()),
            warnings: vec![],
        }],
    );

    assert!(config.providers.is_empty());
    assert_eq!(result.added_providers, 0);
    assert_eq!(result.added_keys, 0);
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("base URL"));
}
