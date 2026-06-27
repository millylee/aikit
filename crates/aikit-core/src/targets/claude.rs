use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use serde_json::Value;

use crate::{AikitError, Result};

use super::{
    backup::{backup_file, backup_file_to_root},
    TargetSelection, TargetWriteResult, TargetWriter,
};

pub struct ClaudeWriter;

impl ClaudeWriter {
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
        let mut value = if path.exists() {
            let existing = fs::read_to_string(path)?;
            serde_json::from_str::<Value>(&existing).map_err(|err| {
                AikitError::TargetWrite(format!("invalid claude json config: {err}"))
            })?
        } else {
            serde_json::json!({})
        };
        if !value.is_object() {
            return Err(AikitError::TargetWrite(
                "claude json config root must be an object".into(),
            ));
        }

        let backup_path = match backup_root {
            Some(root) => backup_file_to_root("claude", path, root)?,
            None => backup_file("claude", path)?,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let object = value.as_object_mut().ok_or_else(|| {
            AikitError::TargetWrite("claude json config root must be an object".into())
        })?;
        let env = object.entry("env").or_insert_with(|| serde_json::json!({}));
        let env_object = env
            .as_object_mut()
            .ok_or_else(|| AikitError::TargetWrite("claude env config must be an object".into()))?;
        env_object.insert(
            "ANTHROPIC_AUTH_TOKEN".into(),
            Value::String(selection.api_key.clone()),
        );
        env_object.insert(
            "ANTHROPIC_BASE_URL".into(),
            Value::String(selection.base_url.clone()),
        );
        env_object.insert(
            "ANTHROPIC_MODEL".into(),
            Value::String(selection.model.clone()),
        );

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
        Ok(dirs.home_dir().join(".claude").join("settings.json"))
    }

    fn write(&self, selection: &TargetSelection) -> Result<TargetWriteResult> {
        Self::write_to_path(&self.default_path()?, selection)
    }
}
