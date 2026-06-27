use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::BaseDirs;

use crate::{AikitError, Result};

use super::{backup::backup_file, TargetSelection, TargetWriteResult, TargetWriter};

pub struct CodexWriter;

impl CodexWriter {
    pub fn write_to_path(path: &Path, selection: &TargetSelection) -> Result<TargetWriteResult> {
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

        let backup_path = backup_file(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        root.insert("model".into(), toml::Value::String(selection.model.clone()));
        root.insert("model_provider".into(), toml::Value::String("aikit".into()));

        let mut provider = toml::map::Map::new();
        provider.insert("name".into(), toml::Value::String("aikit".into()));
        provider.insert(
            "base_url".into(),
            toml::Value::String(selection.base_url.clone()),
        );
        provider.insert(
            "api_key".into(),
            toml::Value::String(selection.api_key.clone()),
        );

        let mut model_providers = match root.remove("model_providers") {
            Some(toml::Value::Table(table)) => table,
            Some(_) | None => toml::map::Map::new(),
        };
        model_providers.insert("aikit".into(), toml::Value::Table(provider));
        root.insert(
            "model_providers".into(),
            toml::Value::Table(model_providers),
        );

        let content = toml::to_string(&toml::Value::Table(root)).map_err(|err| {
            AikitError::TargetWrite(format!("failed to serialize codex config: {err}"))
        })?;
        fs::write(path, content)?;

        Ok(TargetWriteResult {
            target_id: "codex".into(),
            config_path: path.to_path_buf(),
            backup_path,
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
