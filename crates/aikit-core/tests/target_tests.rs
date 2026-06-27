use aikit_core::targets::{claude::ClaudeWriter, codex::CodexWriter, gemini::GeminiWriter, TargetSelection};
use tempfile::tempdir;

#[test]
fn codex_writer_creates_backup_before_writing_existing_config() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    let result = CodexWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "model-new".into(),
        },
    )
    .unwrap();

    assert!(result.backup_path.unwrap().exists());
    let updated = std::fs::read_to_string(path).unwrap();
    assert!(updated.contains("model-new"));
    assert!(updated.contains("https://example.com/v1"));
    assert!(updated.contains("sk-new"));
}

#[test]
fn codex_writer_creates_missing_config() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".codex").join("config.toml");

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
    assert_eq!(
        provider.get("name").and_then(|v| v.as_str()),
        Some("aikit")
    );
}

#[test]
fn claude_writer_creates_minimal_json_config() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

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
    assert_eq!(value["aikit"]["model"], "claude-model");
    assert_eq!(value["aikit"]["base_url"], "https://example.com/v1");
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
    assert!(std::fs::read_to_string(path).unwrap().contains("{invalid json"));
}

#[test]
fn gemini_writer_preserves_existing_json_and_creates_backup() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"existing": true}"#).unwrap();

    let result = GeminiWriter::write_to_path(
        &path,
        &TargetSelection {
            base_url: "https://example.com/v1".into(),
            api_key: "sk-new".into(),
            model: "gemini-model".into(),
        },
    )
    .unwrap();

    assert!(result.backup_path.unwrap().exists());
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value["existing"], true);
    assert_eq!(value["aikit"]["model"], "gemini-model");
    assert_eq!(value["aikit"]["api_key"], "sk-new");
}
