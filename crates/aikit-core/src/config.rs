use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::{AikitError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AikitConfig {
    pub providers: Vec<ProviderConfig>,
    pub active_selection: Option<ActiveSelection>,
    #[serde(default)]
    pub import_prompt: ImportPromptState,
    pub targets: Vec<TargetConfig>,
    pub backup_history: Vec<BackupRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub api_keys: Vec<ApiKeyConfig>,
    #[serde(default)]
    pub manual_models: Vec<String>,
    pub models_cache: Option<ModelCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeyConfig {
    pub id: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCache {
    pub refreshed_at: String,
    pub models: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveSelection {
    pub provider_id: String,
    pub api_key_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ImportPromptState {
    pub skipped_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetConfig {
    pub id: String,
    pub enabled: bool,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupRecord {
    pub target_id: String,
    pub backup_path: PathBuf,
    pub written_at: String,
    pub status: String,
}

impl Default for AikitConfig {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            active_selection: None,
            import_prompt: ImportPromptState::default(),
            targets: vec![
                TargetConfig {
                    id: "claude".into(),
                    enabled: true,
                    config_path: None,
                },
                TargetConfig {
                    id: "gemini".into(),
                    enabled: true,
                    config_path: None,
                },
                TargetConfig {
                    id: "codex".into(),
                    enabled: true,
                    config_path: None,
                },
            ],
            backup_history: Vec::new(),
        }
    }
}

impl AikitConfig {
    pub fn load_from(path: &Path) -> Result<Self> {
        let data = fs::read_to_string(path)?;
        toml::from_str(&data).map_err(|err| AikitError::ConfigParse(err.to_string()))
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data =
            toml::to_string_pretty(self).map_err(|err| AikitError::ConfigParse(err.to_string()))?;
        fs::write(path, data)?;
        set_owner_only(path)?;
        Ok(())
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    let dirs = BaseDirs::new()
        .ok_or_else(|| AikitError::ConfigParse("could not determine config directory".into()))?;
    Ok(dirs.home_dir().join(".aikit").join("config.toml"))
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<()> {
    Ok(())
}
