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
        if path.exists() {
            let existing = fs::read_to_string(path)?;
            toml::from_str::<toml::Value>(&existing).map_err(|err| {
                AikitError::TargetWrite(format!("invalid codex toml config: {err}"))
            })?;
        }

        let backup_path = backup_file(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = format!(
            "model = \"{}\"\nmodel_provider = \"aikit\"\n\n[model_providers.aikit]\nname = \"aikit\"\nbase_url = \"{}\"\napi_key = \"{}\"\n\n",
            selection.model, selection.base_url, selection.api_key
        );
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
