use aikit_core::import::{candidate_fingerprint, scan_env, ImportSource};

#[test]
fn env_scan_imports_openai_key_base_url_and_model() {
    let plan = scan_env([
        ("OPENAI_API_KEY".to_string(), "sk-openai".to_string()),
        ("OPENAI_BASE_URL".to_string(), "https://api.openai.com/v1".to_string()),
        ("OPENAI_MODEL".to_string(), "gpt-4.1-mini".to_string()),
    ]);

    assert!(plan.warnings.is_empty());
    assert_eq!(plan.candidates.len(), 1);
    let candidate = &plan.candidates[0];
    assert_eq!(candidate.source, ImportSource::Env);
    assert_eq!(candidate.provider_id, "openai");
    assert_eq!(candidate.provider_name, "OpenAI");
    assert_eq!(candidate.base_url.as_deref(), Some("https://api.openai.com/v1"));
    assert_eq!(candidate.api_key_name.as_deref(), Some("OPENAI_API_KEY"));
    assert_eq!(candidate.api_key_value.as_deref(), Some("sk-openai"));
    assert_eq!(candidate.model.as_deref(), Some("gpt-4.1-mini"));
}

#[test]
fn env_scan_imports_anthropic_model_variable() {
    let plan = scan_env([
        ("ANTHROPIC_API_KEY".to_string(), "sk-ant".to_string()),
        ("ANTHROPIC_BASE_URL".to_string(), "https://anthropic-proxy.example/v1".to_string()),
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
