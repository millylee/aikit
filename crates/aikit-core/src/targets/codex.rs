use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(all(windows, not(test)))]
use std::process::Command;

use directories::BaseDirs;

use crate::{AikitError, Result};

use super::{
    backup::{backup_file, backup_file_to_root},
    detect::{ensure_tool_present_for_new_config, resolve_tool_config_dir},
    TargetSelection, TargetWriteResult, TargetWriter,
};

pub struct CodexWriter;

impl CodexWriter {
    pub fn write_to_path_with_backup_root(
        path: &Path,
        selection: &TargetSelection,
        backup_root: &Path,
    ) -> Result<TargetWriteResult> {
        Self::write_to_path_inner(path, selection, Some(backup_root))
    }

    fn write_to_path_inner(
        path: &Path,
        selection: &TargetSelection,
        backup_root: Option<&Path>,
    ) -> Result<TargetWriteResult> {
        let dirs = BaseDirs::new()
            .ok_or_else(|| AikitError::TargetWrite("could not determine home directory".into()))?;
        let tool_dir = resolve_tool_config_dir("codex", path, dirs.home_dir())
            .ok_or_else(|| AikitError::TargetWrite("unknown codex config directory".into()))?;
        ensure_tool_present_for_new_config("codex", path, &tool_dir)?;

        let mut root = if path.exists() {
            let existing = fs::read_to_string(path)?;
            match toml::from_str::<toml::Value>(&existing).map_err(|err| {
                AikitError::TargetWrite(format!("invalid codex toml config: {err}"))
            })? {
                toml::Value::Table(table) => table,
                _ => {
                    return Err(AikitError::TargetWrite(
                        "codex toml config must be a root table".into(),
                    ))
                }
            }
        } else {
            toml::map::Map::new()
        };

        let config_backup_path = match backup_root {
            Some(root) => backup_file_to_root("codex", path, root)?,
            None => backup_file("codex", path)?,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        root.insert("model".into(), toml::Value::String(selection.model.clone()));
        root.insert("model_provider".into(), toml::Value::String("aikit".into()));

        if selection.context_1m {
            root.insert(
                "model_context_window".into(),
                toml::Value::Integer(1_000_000),
            );
            root.insert(
                "model_auto_compact_token_limit".into(),
                toml::Value::Integer(900_000),
            );
        } else {
            root.remove("model_context_window");
            root.remove("model_auto_compact_token_limit");
        }

        if selection.max_thinking_effort {
            root.insert(
                "model_reasoning_effort".into(),
                toml::Value::String("max".into()),
            );
        } else if root
            .get("model_reasoning_effort")
            .and_then(toml::Value::as_str)
            == Some("max")
        {
            root.remove("model_reasoning_effort");
        }

        if selection.bypass_permissions {
            root.insert(
                "approval_policy".into(),
                toml::Value::String("never".into()),
            );
            root.insert(
                "sandbox_mode".into(),
                toml::Value::String("danger-full-access".into()),
            );
        } else {
            if root.get("approval_policy").and_then(toml::Value::as_str) == Some("never") {
                root.remove("approval_policy");
            }
            if root.get("sandbox_mode").and_then(toml::Value::as_str) == Some("danger-full-access")
            {
                root.remove("sandbox_mode");
            }
        }

        let mut analytics = match root.remove("analytics") {
            Some(toml::Value::Table(table)) => table,
            Some(_) => {
                return Err(AikitError::TargetWrite(
                    "codex analytics must be a table".into(),
                ))
            }
            None => toml::map::Map::new(),
        };
        if selection.disable_telemetry {
            analytics.insert("enabled".into(), toml::Value::Boolean(false));
        } else if analytics.get("enabled") == Some(&toml::Value::Boolean(false)) {
            analytics.remove("enabled");
        }
        if analytics.is_empty() {
            root.remove("analytics");
        } else {
            root.insert("analytics".into(), toml::Value::Table(analytics));
        }

        let mut model_providers = match root.remove("model_providers") {
            Some(toml::Value::Table(table)) => table,
            Some(_) => {
                return Err(AikitError::TargetWrite(
                    "codex model_providers must be a table".into(),
                ))
            }
            None => toml::map::Map::new(),
        };
        if model_providers
            .get("aikit")
            .is_some_and(|value| !value.is_table())
        {
            return Err(AikitError::TargetWrite(
                "codex model_providers.aikit must be a table".into(),
            ));
        }

        let mut provider = model_providers
            .remove("aikit")
            .and_then(|value| value.as_table().cloned())
            .unwrap_or_default();
        provider.insert("name".into(), toml::Value::String("aikit".into()));
        provider.insert(
            "base_url".into(),
            toml::Value::String(selection.base_url.clone()),
        );
        provider.remove("api_key");
        provider.insert(
            "env_key".into(),
            toml::Value::String("AIKIT_API_KEY".into()),
        );
        model_providers.insert("aikit".into(), toml::Value::Table(provider));
        root.insert(
            "model_providers".into(),
            toml::Value::Table(model_providers),
        );

        let should_persist_env = path == dirs.home_dir().join(".codex").join("config.toml");
        set_aikit_api_key_env(&selection.api_key, should_persist_env, dirs.home_dir());

        if let Some(parent) = path.parent() {
            let auth_path = parent.join("auth.json");
            let mut auth_val = if auth_path.exists() {
                let existing = fs::read_to_string(&auth_path)?;
                serde_json::from_str::<serde_json::Value>(&existing)
                    .unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            if !auth_val.is_object() {
                auth_val = serde_json::json!({});
            }
            if let Some(obj) = auth_val.as_object_mut() {
                obj.insert(
                    "OPENAI_API_KEY".into(),
                    serde_json::Value::String(selection.api_key.clone()),
                );
            }
            let auth_content = serde_json::to_string_pretty(&auth_val).map_err(|err| {
                AikitError::TargetWrite(format!("failed to serialize codex auth config: {err}"))
            })?;
            fs::write(auth_path, auth_content)?;
        }

        let content = toml::to_string(&toml::Value::Table(root)).map_err(|err| {
            AikitError::TargetWrite(format!("failed to serialize codex config: {err}"))
        })?;
        fs::write(path, content)?;

        Ok(TargetWriteResult {
            target_id: "codex".into(),
            config_path: path.to_path_buf(),
            backup_path: config_backup_path,
        })
    }
}

fn set_aikit_api_key_env(api_key: &str, persist_user_scope: bool, _home_dir: &Path) {
    std::env::set_var("AIKIT_API_KEY", api_key);

    if persist_user_scope {
        #[cfg(all(windows, not(test)))]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let Ok(_status) = Command::new("setx")
                .arg("AIKIT_API_KEY")
                .arg(api_key)
                .creation_flags(CREATE_NO_WINDOW)
                .status()
            else {
                return;
            };
        }

        #[cfg(all(unix, not(test)))]
        {
            persist_unix_aikit_api_key(api_key, _home_dir);
        }
    }
}

#[cfg(all(unix, not(test)))]
fn persist_unix_aikit_api_key(api_key: &str, home_dir: &Path) {
    let rc_path = preferred_unix_shell_rc(home_dir);
    let existing = fs::read_to_string(&rc_path).unwrap_or_default();

    let begin = "# >>> aikit AIKIT_API_KEY >>>";
    let end = "# <<< aikit AIKIT_API_KEY <<<";
    let escaped = api_key.replace('\'', "'\\''");
    let block = format!(
        "{begin}\nexport AIKIT_API_KEY='{escaped}'\n{end}\n",
        begin = begin,
        escaped = escaped,
        end = end
    );

    let updated = if let (Some(start), Some(finish)) = (existing.find(begin), existing.find(end)) {
        if finish >= start {
            let end_index = finish + end.len();
            let mut value = String::new();
            value.push_str(&existing[..start]);
            if !value.ends_with('\n') && !value.is_empty() {
                value.push('\n');
            }
            value.push_str(&block);
            let suffix = existing[end_index..].trim_start_matches('\n');
            if !suffix.is_empty() {
                value.push_str(suffix);
                if !value.ends_with('\n') {
                    value.push('\n');
                }
            }
            value
        } else {
            append_block(&existing, &block)
        }
    } else {
        append_block(&existing, &block)
    };

    let _ = fs::write(rc_path, updated);
}

#[cfg(all(unix, not(test)))]
fn append_block(existing: &str, block: &str) -> String {
    if existing.is_empty() {
        return block.to_string();
    }

    let mut value = existing.to_string();
    if !value.ends_with('\n') {
        value.push('\n');
    }
    value.push_str(block);
    value
}

#[cfg(all(unix, not(test)))]
fn preferred_unix_shell_rc(home_dir: &Path) -> PathBuf {
    let shell = std::env::var("SHELL").unwrap_or_default();
    let shell_name = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    match shell_name {
        "zsh" => home_dir.join(".zshrc"),
        "bash" => home_dir.join(".bashrc"),
        _ => home_dir.join(".profile"),
    }
}

impl TargetWriter for CodexWriter {
    fn target_id(&self) -> &'static str {
        "codex"
    }

    fn default_path(&self) -> Result<PathBuf> {
        let dirs = BaseDirs::new()
            .ok_or_else(|| AikitError::TargetWrite("could not determine home directory".into()))?;
        Ok(dirs.home_dir().join(".codex").join("config.toml"))
    }

    fn write(&self, selection: &TargetSelection) -> Result<TargetWriteResult> {
        Self::write_to_path_inner(&self.default_path()?, selection, None)
    }
}
