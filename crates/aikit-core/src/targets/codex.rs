use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::BaseDirs;

use crate::{AikitError, Result};

use super::{
    backup::{backup_file, backup_file_to_root},
    detect::{ensure_tool_present_for_new_config, resolve_tool_config_dir},
    TargetSelection, TargetWriteResult, TargetWriter,
};

pub struct CodexWriter;

impl CodexWriter {
    pub fn write_to_path(path: &Path, selection: &TargetSelection) -> Result<TargetWriteResult> {
        Self::write_to_path_inner(path, selection, None)
    }

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

        let mut env = match root.remove("env") {
            Some(toml::Value::Table(table)) => table,
            Some(_) => {
                return Err(AikitError::TargetWrite(
                    "codex env must be a table".into(),
                ))
            }
            None => toml::map::Map::new(),
        };
        env.insert(
            "AIKIT_API_KEY".into(),
            toml::Value::String(selection.api_key.clone()),
        );
        root.insert("env".into(), toml::Value::Table(env));

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
        Self::write_to_path(&self.default_path()?, selection)
    }
}
