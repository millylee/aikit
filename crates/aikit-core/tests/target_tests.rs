use aikit_core::targets::{claude::ClaudeWriter, codex::CodexWriter, TargetSelection};
use aikit_core::AikitError;
use tempfile::tempdir;

#[test]
fn codex_writer_creates_backup_before_writing_existing_config() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let backup_root = dir.path().join("aikit");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    let result = CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        &backup_root,
    )
    .unwrap();

    let backup_path = result.backup_path.unwrap();
    assert!(backup_path.exists());
    assert!(backup_path.starts_with(backup_root.join("backups").join("codex")));
    assert!(backup_root.join("logs").join("backups.jsonl").exists());
    let updated = std::fs::read_to_string(path).unwrap();
    assert!(updated.contains("model-new"));
    assert!(updated.contains("https://example.com/v1"));
    let parsed: toml::Value = toml::from_str(&updated).unwrap();
    let provider = parsed.get("model_providers").and_then(|v| v.get("aikit"));
    assert_eq!(
        provider
            .and_then(|v| v.get("env_key"))
            .and_then(|v| v.as_str()),
        Some("AIKIT_API_KEY")
    );

    let auth_path = dir.path().join("auth.json");
    assert!(auth_path.exists());
    let auth_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(auth_path).unwrap()).unwrap();
    assert_eq!(
        auth_json.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
        Some("sk-new")
    );
}

#[test]
fn codex_writer_creates_missing_config() {
    let dir = tempdir().unwrap();
    let tool_dir = dir.path().join(".codex");
    std::fs::create_dir_all(&tool_dir).unwrap();
    let path = tool_dir.join("config.toml");

    let result = CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    assert!(result.backup_path.is_none());
    assert!(path.exists());
    let updated = std::fs::read_to_string(path).unwrap();
    assert!(updated.contains("model-new"));
    let parsed: toml::Value = toml::from_str(&updated).unwrap();
    let provider = parsed.get("model_providers").and_then(|v| v.get("aikit"));
    assert_eq!(
        provider
            .and_then(|v| v.get("env_key"))
            .and_then(|v| v.as_str()),
        Some("AIKIT_API_KEY")
    );

    let auth_path = tool_dir.join("auth.json");
    assert!(auth_path.exists());
    let auth_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(auth_path).unwrap()).unwrap();
    assert_eq!(
        auth_json.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
        Some("sk-new")
    );
}

#[test]
fn codex_writer_skips_missing_config_when_tool_dir_absent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".codex").join("config.toml");

    let result = CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    );

    assert!(matches!(result, Err(AikitError::TargetSkipped(_))));
    assert!(!path.exists());
}

#[test]
fn codex_writer_updates_existing_config_when_tool_dir_absent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let updated = std::fs::read_to_string(path).unwrap();
    assert!(updated.contains("model-new"));
}

#[test]
fn codex_writer_refuses_invalid_existing_toml() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "not = [valid").unwrap();

    let result = CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    );

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "not = [valid");
}

#[test]
fn codex_writer_serializes_special_characters_in_toml() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    let selection = TargetSelection {
        base_url: "https://example.com/v1?ref=\"test\"".into(),
        api_key: "sk\\key\"quoted".into(),
        model: "model\\with\"quotes".into(),
        claude_pin_models: false,
        context_1m: false,
        bypass_permissions: false,
        max_thinking_effort: false,
        claude_disable_betas: false,
        claude_disable_autoupdater: false,
        disable_telemetry: false,
    };

    CodexWriter::write_to_path_with_backup_root(&path, &selection, dir.path()).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&content).unwrap();

    assert_eq!(
        parsed.get("model").and_then(|v| v.as_str()),
        Some(selection.model.as_str())
    );
    assert_eq!(
        parsed.get("model_provider").and_then(|v| v.as_str()),
        Some("aikit")
    );

    let provider = parsed
        .get("model_providers")
        .and_then(|v| v.get("aikit"))
        .expect("model_providers.aikit table");
    assert_eq!(
        provider.get("base_url").and_then(|v| v.as_str()),
        Some(selection.base_url.as_str())
    );
    assert!(provider.get("api_key").is_none());
    assert_eq!(
        provider.get("env_key").and_then(|v| v.as_str()),
        Some("AIKIT_API_KEY")
    );
    assert_eq!(provider.get("name").and_then(|v| v.as_str()), Some("aikit"));

    let auth_path = dir.path().join("auth.json");
    assert!(auth_path.exists());
    let auth_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(auth_path).unwrap()).unwrap();
    assert_eq!(
        auth_json.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
        Some(selection.api_key.as_str())
    );
}

#[test]
fn codex_writer_enables_1m_context_with_window_settings() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: true,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        &dir.path().join("aikit"),
    )
    .unwrap();

    let updated = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&updated).unwrap();
    assert_eq!(
        parsed
            .get("model_context_window")
            .and_then(toml::Value::as_integer),
        Some(1_000_000)
    );
    assert_eq!(
        parsed
            .get("model_auto_compact_token_limit")
            .and_then(toml::Value::as_integer),
        Some(900_000)
    );
}

#[test]
fn codex_writer_disables_1m_context_by_removing_window_settings() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
model = "old"
model_context_window = 1000000
model_auto_compact_token_limit = 900000
"#,
    )
    .unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        &dir.path().join("aikit"),
    )
    .unwrap();

    let updated = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&updated).unwrap();
    assert!(parsed.get("model_context_window").is_none());
    assert!(parsed.get("model_auto_compact_token_limit").is_none());
}

#[test]
fn codex_writer_preserves_unrelated_existing_toml_keys() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
model = "old-model"
approval_policy = "on-request"

[model_providers.other]
name = "other"
base_url = "https://other.example/v1"

[profiles.default]
model = "keep-me"
"#,
    )
    .unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        &dir.path().join("aikit"),
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&content).unwrap();

    assert_eq!(
        parsed.get("approval_policy").and_then(|v| v.as_str()),
        Some("on-request")
    );
    assert_eq!(
        parsed
            .get("model_providers")
            .and_then(|v| v.get("other"))
            .and_then(|v| v.get("base_url"))
            .and_then(|v| v.as_str()),
        Some("https://other.example/v1")
    );
    assert_eq!(
        parsed
            .get("profiles")
            .and_then(|v| v.get("default"))
            .and_then(|v| v.get("model"))
            .and_then(|v| v.as_str()),
        Some("keep-me")
    );
    assert_eq!(
        parsed.get("model").and_then(|v| v.as_str()),
        Some("model-new")
    );
    let auth_path = dir.path().join("auth.json");
    assert!(auth_path.exists());
    let auth_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(auth_path).unwrap()).unwrap();
    assert_eq!(
        auth_json.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
        Some("sk-new")
    );
}

#[test]
fn codex_writer_refuses_non_table_model_providers_and_preserves_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = r#"
model = "old-model"
model_providers = "not-a-table"
"#;
    std::fs::write(&path, original).unwrap();

    let result = CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    );

    assert!(matches!(result, Err(AikitError::TargetWrite(_))));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn codex_writer_refuses_non_table_aikit_provider_and_preserves_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = r#"
model = "old-model"

[model_providers]
aikit = "not-a-table"
"#;
    std::fs::write(&path, original).unwrap();

    let result = CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    );

    assert!(matches!(result, Err(AikitError::TargetWrite(_))));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn claude_writer_creates_minimal_json_config() {
    let dir = tempdir().unwrap();
    let tool_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&tool_dir).unwrap();
    let path = tool_dir.join("settings.json");

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let updated = std::fs::read_to_string(path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&updated).unwrap();
    assert_eq!(value["model"], "claude-model");
    assert!(value["env"].get("ANTHROPIC_MODEL").is_none());
    assert_eq!(value["env"]["ANTHROPIC_BASE_URL"], "https://example.com/v1");
    assert_eq!(value["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-new");
}

#[test]
fn claude_writer_skips_missing_config_when_tool_dir_absent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".claude").join("settings.json");

    let result = ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    );

    assert!(matches!(result, Err(AikitError::TargetSkipped(_))));
    assert!(!path.exists());
}

#[test]
fn claude_writer_preserves_existing_json_and_writes_native_env() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"theme":"dark","env":{"KEEP":"yes","ANTHROPIC_MODEL":"old"}}"#,
    )
    .unwrap();

    let backup_root = dir.path().join("aikit");
    let result = ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        &backup_root,
    )
    .unwrap();

    assert!(result.backup_path.unwrap().exists());
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["env"]["KEEP"], "yes");
    assert_eq!(value["model"], "claude-model");
    assert!(value["env"].get("ANTHROPIC_MODEL").is_none());
    assert_eq!(value["env"]["ANTHROPIC_BASE_URL"], "https://example.com/v1");
    assert_eq!(value["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-new");
}

#[test]
fn claude_writer_refuses_json_array_root_and_preserves_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let original = r#"[{"existing": true}]"#;
    std::fs::write(&path, original).unwrap();

    let result = ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    );

    assert!(matches!(result, Err(AikitError::TargetWrite(_))));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn claude_writer_pins_all_model_env_vars_when_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"theme":"dark","env":{"KEEP":"yes"}}"#).unwrap();

    let backup_root = dir.path().join("aikit");
    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "glm-5.2".into(),
            claude_pin_models: true,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        &backup_root,
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["model"], "glm-5.2");
    assert_eq!(value["env"]["KEEP"], "yes");
    assert_eq!(value["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-new");
    assert_eq!(value["env"]["ANTHROPIC_BASE_URL"], "https://example.com/v1");
    for var in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_SMALL_FAST_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ] {
        assert_eq!(value["env"][var], "glm-5.2", "expected {var} to be pinned");
    }
    assert!(value["env"]
        .get("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
        .is_none());
}

#[test]
fn claude_writer_applies_1m_suffix_and_compact_window() {
    let dir = tempdir().unwrap();
    let tool_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&tool_dir).unwrap();
    let path = tool_dir.join("settings.json");

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "glm-5.2".into(),
            claude_pin_models: true,
            context_1m: true,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["model"], "glm-5.2[1m]");
    assert_eq!(value["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "1000000");
    for var in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_SMALL_FAST_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ] {
        assert_eq!(
            value["env"][var], "glm-5.2[1m]",
            "expected {var} pinned with 1m suffix"
        );
    }
}

#[test]
fn claude_writer_does_not_double_suffix_already_suffixed_model() {
    let dir = tempdir().unwrap();
    let tool_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&tool_dir).unwrap();
    let path = tool_dir.join("settings.json");

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "glm-5.2[1m]".into(),
            claude_pin_models: false,
            context_1m: true,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["model"], "glm-5.2[1m]");
    assert_eq!(value["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "1000000");
    assert!(value["env"].get("ANTHROPIC_MODEL").is_none());
}

#[test]
fn claude_writer_disables_pin_and_compact_cleans_stale_vars() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"env":{
            "ANTHROPIC_MODEL":"old",
            "ANTHROPIC_SMALL_FAST_MODEL":"old",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL":"old",
            "ANTHROPIC_DEFAULT_SONNET_MODEL":"old",
            "ANTHROPIC_DEFAULT_OPUS_MODEL":"old",
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW":"999",
            "KEEP":"yes"
        }}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "glm-5.2".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["model"], "glm-5.2");
    assert_eq!(value["env"]["KEEP"], "yes");
    for var in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_SMALL_FAST_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
    ] {
        assert!(
            value["env"].get(var).is_none(),
            "expected {var} removed when disabled"
        );
    }
}

#[test]
fn claude_writer_sets_bypass_permissions_when_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"theme":"dark","permissions":{"allow":["Bash(ls:*)"]}}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: true,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["permissions"]["defaultMode"], "bypassPermissions");
    assert_eq!(value["permissions"]["allow"][0], "Bash(ls:*)");
    assert_eq!(value["theme"], "dark");
}

#[test]
fn claude_writer_removes_bypass_permissions_when_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"permissions":{"defaultMode":"bypassPermissions","allow":["Bash(ls:*)"]}}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(value["permissions"].get("defaultMode").is_none());
    assert_eq!(value["permissions"]["allow"][0], "Bash(ls:*)");
}

#[test]
fn claude_writer_preserves_custom_permission_mode_when_bypass_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"permissions":{"defaultMode":"acceptEdits"}}"#).unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["permissions"]["defaultMode"], "acceptEdits");
}

#[test]
fn codex_writer_sets_bypass_permissions_when_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: true,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        parsed.get("approval_policy").and_then(|v| v.as_str()),
        Some("never")
    );
    assert_eq!(
        parsed.get("sandbox_mode").and_then(|v| v.as_str()),
        Some("danger-full-access")
    );
}

#[test]
fn codex_writer_removes_bypass_permissions_when_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "model = \"old\"\napproval_policy = \"never\"\nsandbox_mode = \"danger-full-access\"\n",
    )
    .unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(parsed.get("approval_policy").is_none());
    assert!(parsed.get("sandbox_mode").is_none());
}

#[test]
fn codex_writer_preserves_custom_approval_policy_when_bypass_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\napproval_policy = \"untrusted\"\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        parsed.get("approval_policy").and_then(|v| v.as_str()),
        Some("untrusted")
    );
}

#[test]
fn codex_writer_sets_max_reasoning_effort_when_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: true,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        parsed
            .get("model_reasoning_effort")
            .and_then(|v| v.as_str()),
        Some("max")
    );
}

#[test]
fn codex_writer_removes_aikit_max_reasoning_effort_when_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\nmodel_reasoning_effort = \"max\"\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(parsed.get("model_reasoning_effort").is_none());
}

#[test]
fn codex_writer_preserves_custom_reasoning_effort_when_option_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "model = \"old\"\nmodel_reasoning_effort = \"medium\"\n",
    )
    .unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        parsed
            .get("model_reasoning_effort")
            .and_then(|v| v.as_str()),
        Some("medium")
    );
}

#[test]
fn claude_writer_sets_max_effort_env_when_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"theme":"dark","env":{"KEEP":"yes"}}"#).unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: true,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["env"]["CLAUDE_CODE_EFFORT_LEVEL"], "max");
    assert_eq!(value["env"]["CLAUDE_CODE_ALWAYS_ENABLE_EFFORT"], "1");
    assert_eq!(value["env"]["KEEP"], "yes");
    assert_eq!(value["theme"], "dark");
}

#[test]
fn claude_writer_removes_aikit_max_effort_env_when_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"env":{
            "CLAUDE_CODE_EFFORT_LEVEL":"max",
            "CLAUDE_CODE_ALWAYS_ENABLE_EFFORT":"1",
            "KEEP":"yes"
        }}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(value["env"].get("CLAUDE_CODE_EFFORT_LEVEL").is_none());
    assert!(value["env"]
        .get("CLAUDE_CODE_ALWAYS_ENABLE_EFFORT")
        .is_none());
    assert_eq!(value["env"]["KEEP"], "yes");
}

#[test]
fn claude_writer_preserves_custom_effort_env_when_option_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"env":{"CLAUDE_CODE_EFFORT_LEVEL":"high","KEEP":"yes"}}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["env"]["CLAUDE_CODE_EFFORT_LEVEL"], "high");
    assert_eq!(value["env"]["KEEP"], "yes");
}

#[test]
fn claude_writer_sets_disable_betas_env_when_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"theme":"dark","env":{"KEEP":"yes"}}"#).unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: true,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["env"]["CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS"], "1");
    assert_eq!(value["env"]["KEEP"], "yes");
    assert_eq!(value["theme"], "dark");
}

#[test]
fn claude_writer_removes_aikit_disable_betas_env_when_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"env":{
            "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS":"1",
            "KEEP":"yes"
        }}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(value["env"]
        .get("CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS")
        .is_none());
    assert_eq!(value["env"]["KEEP"], "yes");
}

#[test]
fn claude_writer_preserves_custom_disable_betas_env_when_option_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"env":{"CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS":"0","KEEP":"yes"}}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["env"]["CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS"], "0");
    assert_eq!(value["env"]["KEEP"], "yes");
}

#[test]
fn claude_writer_sets_disable_autoupdater_env_when_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"theme":"dark","env":{"KEEP":"yes"}}"#).unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: true,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["env"]["DISABLE_AUTOUPDATER"], "1");
    assert_eq!(value["env"]["KEEP"], "yes");
    assert_eq!(value["theme"], "dark");
}

#[test]
fn claude_writer_removes_aikit_disable_autoupdater_env_when_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"env":{
            "DISABLE_AUTOUPDATER":"1",
            "KEEP":"yes"
        }}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(value["env"].get("DISABLE_AUTOUPDATER").is_none());
    assert_eq!(value["env"]["KEEP"], "yes");
}

#[test]
fn claude_writer_keeps_user_disable_autoupdater_value_when_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"env":{"DISABLE_AUTOUPDATER":"0"}}"#).unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["env"]["DISABLE_AUTOUPDATER"], "0");
}

#[test]
fn claude_writer_sets_disable_telemetry_env_when_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"theme":"dark","env":{"KEEP":"yes"}}"#).unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: true,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        value["env"]["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"],
        "1"
    );
    assert_eq!(value["env"]["KEEP"], "yes");
    assert_eq!(value["theme"], "dark");
}

#[test]
fn claude_writer_removes_aikit_disable_telemetry_env_when_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"env":{
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC":"1",
            "KEEP":"yes"
        }}"#,
    )
    .unwrap();

    ClaudeWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(value["env"]
        .get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
        .is_none());
    assert_eq!(value["env"]["KEEP"], "yes");
}

#[test]
fn codex_writer_sets_analytics_disabled_when_telemetry_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: true,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        parsed
            .get("analytics")
            .and_then(|v| v.get("enabled"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn codex_writer_keeps_other_analytics_keys_when_telemetry_disabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[analytics]\nsample_rate = 0.5\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: true,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        parsed
            .get("analytics")
            .and_then(|v| v.get("enabled"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        parsed
            .get("analytics")
            .and_then(|v| v.get("sample_rate"))
            .and_then(|v| v.as_float()),
        Some(0.5)
    );
}

#[test]
fn codex_writer_removes_aikit_analytics_when_telemetry_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[analytics]\nenabled = false\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(parsed.get("analytics").is_none());
}

#[test]
fn codex_writer_keeps_user_analytics_and_remaining_keys_when_telemetry_enabled() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[analytics]\nenabled = true\nsample_rate = 0.5\n").unwrap();

    CodexWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
            claude_pin_models: false,
            context_1m: false,
            bypass_permissions: false,
            max_thinking_effort: false,
            claude_disable_betas: false,
            claude_disable_autoupdater: false,
            disable_telemetry: false,
        },
        dir.path(),
    )
    .unwrap();

    let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let analytics = parsed.get("analytics").unwrap();
    assert_eq!(
        analytics.get("enabled").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        analytics.get("sample_rate").and_then(|v| v.as_float()),
        Some(0.5)
    );
}
