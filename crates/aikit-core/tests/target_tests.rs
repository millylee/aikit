use aikit_core::targets::{codex::CodexWriter, TargetSelection};
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
