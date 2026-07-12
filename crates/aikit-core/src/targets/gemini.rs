use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use serde_json::Value;

use crate::{AikitError, Result};

use super::{
    backup::{backup_file, backup_file_to_root},
    detect::{ensure_tool_present_for_new_config, resolve_tool_config_dir},
    TargetSelection, TargetWriteResult, TargetWriter,
};

pub struct GeminiWriter;

impl GeminiWriter {
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
        let tool_dir = resolve_tool_config_dir("gemini", path, dirs.home_dir())
            .ok_or_else(|| AikitError::TargetWrite("unknown gemini config directory".into()))?;
        ensure_tool_present_for_new_config("gemini", path, &tool_dir)?;

        let mut value = if path.exists() {
            let existing = fs::read_to_string(path)?;
            serde_json::from_str::<Value>(&existing).map_err(|err| {
                AikitError::TargetWrite(format!("invalid gemini json config: {err}"))
            })?
        } else {
            serde_json::json!({})
        };
        if !value.is_object() {
            return Err(AikitError::TargetWrite(
                "gemini json config root must be an object".into(),
            ));
        }

        let backup_path = match backup_root {
            Some(root) => backup_file_to_root("gemini", path, root)?,
            None => backup_file("gemini", path)?,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        value["aikit"] = serde_json::json!({
            "base_url": selection.base_url,
            "api_key": selection.api_key,
            "model": selection.model,
        });

        let content = serde_json::to_string_pretty(&value).map_err(|err| {
            AikitError::TargetWrite(format!("failed to serialize gemini config: {err}"))
        })?;
        fs::write(path, content)?;

        Ok(TargetWriteResult {
            target_id: "gemini".into(),
            config_path: path.to_path_buf(),
            backup_path,
        })
    }
}

impl TargetWriter for GeminiWriter {
    fn target_id(&self) -> &'static str {
        "gemini"
    }

    fn default_path(&self) -> Result<PathBuf> {
        let dirs = BaseDirs::new()
            .ok_or_else(|| AikitError::TargetWrite("could not determine home directory".into()))?;
        Ok(dirs.home_dir().join(".gemini").join("settings.json"))
    }

    fn write(&self, selection: &TargetSelection) -> Result<TargetWriteResult> {
        Self::write_to_path(&self.default_path()?, selection)
    }
}
