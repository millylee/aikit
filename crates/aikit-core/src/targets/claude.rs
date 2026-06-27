use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use serde_json::Value;

use crate::{AikitError, Result};

use super::{backup::backup_file, TargetSelection, TargetWriteResult, TargetWriter};

pub struct ClaudeWriter;

impl ClaudeWriter {
    pub fn write_to_path(path: &Path, selection: &TargetSelection) -> Result<TargetWriteResult> {
        let mut value = if path.exists() {
            let existing = fs::read_to_string(path)?;
            serde_json::from_str::<Value>(&existing).map_err(|err| {
                AikitError::TargetWrite(format!("invalid claude json config: {err}"))
            })?
        } else {
            serde_json::json!({})
        };

        let backup_path = backup_file(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        value["aikit"] = serde_json::json!({
            "base_url": selection.base_url,
            "api_key": selection.api_key,
            "model": selection.model,
        });

        let content = serde_json::to_string_pretty(&value).map_err(|err| {
            AikitError::TargetWrite(format!("failed to serialize claude config: {err}"))
        })?;
        fs::write(path, content)?;

        Ok(TargetWriteResult {
            target_id: "claude".into(),
            config_path: path.to_path_buf(),
            backup_path,
        })
    }
}

impl TargetWriter for ClaudeWriter {
    fn target_id(&self) -> &'static str {
        "claude"
    }

    fn default_path(&self) -> Result<PathBuf> {
        let dirs = BaseDirs::new()
            .ok_or_else(|| AikitError::TargetWrite("could not determine home directory".into()))?;
        Ok(dirs
            .home_dir()
            .join(".claude")
            .join("settings.json"))
    }

    fn write(&self, selection: &TargetSelection) -> Result<TargetWriteResult> {
        Self::write_to_path(&self.default_path()?, selection)
    }
}
