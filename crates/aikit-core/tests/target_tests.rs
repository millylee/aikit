use aikit_core::targets::{
    claude::ClaudeWriter, codex::CodexWriter, gemini::GeminiWriter, TargetSelection,
};
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
    assert!(updated.contains("sk-new"));
}

#[test]
fn codex_writer_creates_missing_config() {
    let dir = tempdir().unwrap();
    let tool_dir = dir.path().join(".codex");
    std::fs::create_dir_all(&tool_dir).unwrap();
    let path = tool_dir.join("config.toml");

    let result = CodexWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
        },
    )
    .unwrap();

    assert!(result.backup_path.is_none());
    assert!(path.exists());
    let updated = std::fs::read_to_string(path).unwrap();
    assert!(updated.contains("model-new"));
    assert!(updated.contains("sk-new"));
}

#[test]
fn codex_writer_skips_missing_config_when_tool_dir_absent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".codex").join("config.toml");

    let result = CodexWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
        },
    );

    assert!(matches!(result, Err(AikitError::TargetSkipped(_))));
    assert!(!path.exists());
}

#[test]
fn codex_writer_updates_existing_config_when_tool_dir_absent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    CodexWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
        },
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

    let result = CodexWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
        },
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
    };

    CodexWriter::write_to_path(&path, &selection).unwrap();

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
    assert_eq!(
        provider.get("api_key").and_then(|v| v.as_str()),
        Some(selection.api_key.as_str())
    );
    assert_eq!(provider.get("name").and_then(|v| v.as_str()), Some("aikit"));
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
    assert_eq!(
        parsed
            .get("model_providers")
            .and_then(|v| v.get("aikit"))
            .and_then(|v| v.get("api_key"))
            .and_then(|v| v.as_str()),
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

    let result = CodexWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
        },
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

    let result = CodexWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
        },
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

    ClaudeWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
        },
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

    let result = ClaudeWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
        },
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
fn gemini_writer_skips_missing_config_when_tool_dir_absent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".gemini").join("settings.json");

    let result = GeminiWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "gemini-model".into(),
        },
    );

    assert!(matches!(result, Err(AikitError::TargetSkipped(_))));
    assert!(!path.exists());
}

#[test]
fn gemini_writer_creates_minimal_json_config_when_tool_dir_exists() {
    let dir = tempdir().unwrap();
    let tool_dir = dir.path().join(".gemini");
    std::fs::create_dir_all(&tool_dir).unwrap();
    let path = tool_dir.join("settings.json");

    GeminiWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "gemini-model".into(),
        },
    )
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["aikit"]["model"], "gemini-model");
    assert_eq!(value["aikit"]["api_key"], "sk-new");
}

#[test]
fn gemini_writer_refuses_invalid_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{invalid json").unwrap();

    let result = GeminiWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "gemini-model".into(),
        },
    );

    assert!(result.is_err());
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("{invalid json"));
}

#[test]
fn gemini_writer_preserves_existing_json_and_creates_backup() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"existing": true}"#).unwrap();

    let backup_root = dir.path().join("aikit");
    let result = GeminiWriter::write_to_path_with_backup_root(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "gemini-model".into(),
        },
        &backup_root,
    )
    .unwrap();

    assert!(result.backup_path.unwrap().exists());
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["existing"], true);
    assert_eq!(value["aikit"]["model"], "gemini-model");
    assert_eq!(value["aikit"]["api_key"], "sk-new");
}

#[test]
fn claude_writer_refuses_json_array_root_and_preserves_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let original = r#"[{"existing": true}]"#;
    std::fs::write(&path, original).unwrap();

    let result = ClaudeWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "claude-model".into(),
        },
    );

    assert!(matches!(result, Err(AikitError::TargetWrite(_))));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}
