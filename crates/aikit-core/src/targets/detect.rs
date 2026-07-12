use std::path::{Path, PathBuf};

use crate::{AikitError, Result};

const TARGETS: [(&str, &str, &str); 3] = [
    ("claude", ".claude", "Claude Code"),
    ("gemini", ".gemini", "Gemini CLI"),
    ("codex", ".codex", "Codex CLI"),
];

pub fn tool_config_dir(target_id: &str, home: &Path) -> Option<PathBuf> {
    tool_dir_name(target_id).map(|dir_name| home.join(dir_name))
}

pub fn tool_display_name(target_id: &str) -> String {
    TARGETS
        .iter()
        .find(|(id, _, _)| *id == target_id)
        .map(|(_, _, name)| (*name).to_string())
        .unwrap_or_else(|| "Unknown target".to_string())
}

pub fn resolve_tool_config_dir(
    target_id: &str,
    config_path: &Path,
    home: &Path,
) -> Option<PathBuf> {
    if let Some(parent) = config_path.parent() {
        if let Some(dir_name) = tool_dir_name(target_id) {
            if parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == dir_name)
            {
                return Some(parent.to_path_buf());
            }
        }
    }
    tool_config_dir(target_id, home)
}

pub fn ensure_tool_present_for_new_config(
    target_id: &str,
    config_path: &Path,
    tool_config_dir: &Path,
) -> Result<()> {
    if config_path.exists() {
        return Ok(());
    }
    if tool_config_dir.is_dir() {
        return Ok(());
    }

    Err(AikitError::TargetSkipped(format!(
        "{} config directory not found (`{}`); refusing to create config",
        tool_display_name(target_id),
        tool_config_dir.display()
    )))
}

fn tool_dir_name(target_id: &str) -> Option<&'static str> {
    TARGETS
        .iter()
        .find(|(id, _, _)| *id == target_id)
        .map(|(_, dir, _)| *dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ensure_tool_present_skips_check_when_config_exists() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("settings.json");
        std::fs::write(&config_path, "{}").unwrap();

        ensure_tool_present_for_new_config("claude", &config_path, dir.path()).unwrap();
    }

    #[test]
    fn ensure_tool_present_allows_new_config_when_tool_dir_exists() {
        let dir = tempdir().unwrap();
        let tool_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&tool_dir).unwrap();
        let config_path = tool_dir.join("settings.json");

        ensure_tool_present_for_new_config("claude", &config_path, &tool_dir).unwrap();
    }

    #[test]
    fn ensure_tool_present_refuses_new_config_when_tool_dir_missing() {
        let dir = tempdir().unwrap();
        let tool_dir = dir.path().join(".claude");
        let config_path = tool_dir.join("settings.json");

        let err =
            ensure_tool_present_for_new_config("claude", &config_path, &tool_dir).unwrap_err();
        assert!(matches!(err, AikitError::TargetSkipped(_)));
        assert!(!config_path.exists());
    }

    #[test]
    fn resolve_tool_config_dir_uses_parent_when_named_like_tool_dir() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".codex").join("config.toml");

        assert_eq!(
            resolve_tool_config_dir("codex", &config_path, Path::new("/home/user")),
            Some(dir.path().join(".codex"))
        );
    }

    #[test]
    fn resolve_tool_config_dir_falls_back_to_home_tool_dir() {
        let home = Path::new("/home/user");
        let config_path = home.join("custom").join("config.toml");

        assert_eq!(
            resolve_tool_config_dir("codex", &config_path, home),
            Some(home.join(".codex"))
        );
    }

    #[test]
    fn tool_display_name_returns_known_and_unknown_names() {
        assert_eq!(tool_display_name("claude"), "Claude Code");
        assert_eq!(tool_display_name("missing"), "Unknown target");
    }
}
