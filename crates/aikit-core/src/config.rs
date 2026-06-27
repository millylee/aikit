use std::{
    collections::BTreeMap,
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
    #[serde(default, skip_serializing)]
    pub import_prompt: ImportPromptState,
    pub targets: Vec<TargetConfig>,
    #[serde(default, skip_serializing)]
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
    #[serde(default, skip_serializing)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AikitState {
    #[serde(default)]
    pub import_prompt: ImportPromptState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelCacheStore {
    #[serde(default)]
    pub providers: BTreeMap<String, ModelCache>,
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

    pub fn load_with_sidecars(path: &Path) -> Result<Self> {
        let mut config = Self::load_from(path)?;
        load_sidecars(path, &mut config)?;
        Ok(config)
    }

    pub fn save_with_sidecars(&self, path: &Path) -> Result<()> {
        self.save_to(path)?;
        save_sidecars(path, self)
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    let dirs = BaseDirs::new()
        .ok_or_else(|| AikitError::ConfigParse("could not determine config directory".into()))?;
    Ok(dirs.home_dir().join(".aikit").join("config.toml"))
}

pub fn load_sidecars(config_path: &Path, config: &mut AikitConfig) -> Result<()> {
    config.import_prompt = load_state(config_path)?.import_prompt;
    let cache_store = load_model_cache_store(config_path)?;
    for provider in &mut config.providers {
        provider.models_cache = cache_store.providers.get(&provider.id).cloned();
    }
    Ok(())
}

pub fn save_sidecars(config_path: &Path, config: &AikitConfig) -> Result<()> {
    save_state(
        config_path,
        &AikitState {
            import_prompt: config.import_prompt.clone(),
        },
    )?;
    save_model_cache_store(config_path, &model_cache_store_from_config(config))?;
    Ok(())
}

pub fn load_state(config_path: &Path) -> Result<AikitState> {
    let path = state_path(config_path);
    if !path.exists() {
        return Ok(AikitState::default());
    }
    let data = fs::read_to_string(path)?;
    toml::from_str(&data).map_err(|err| AikitError::ConfigParse(err.to_string()))
}

pub fn save_state(config_path: &Path, state: &AikitState) -> Result<()> {
    let path = state_path(config_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data =
        toml::to_string_pretty(state).map_err(|err| AikitError::ConfigParse(err.to_string()))?;
    fs::write(&path, data)?;
    set_owner_only(&path)?;
    Ok(())
}

pub fn load_model_cache_store(config_path: &Path) -> Result<ModelCacheStore> {
    let path = model_cache_path(config_path);
    if !path.exists() {
        return Ok(ModelCacheStore::default());
    }
    let data = fs::read_to_string(path)?;
    serde_json::from_str(&data).map_err(|err| AikitError::ConfigParse(err.to_string()))
}

pub fn save_model_cache_store(config_path: &Path, store: &ModelCacheStore) -> Result<()> {
    let path = model_cache_path(config_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(store)
        .map_err(|err| AikitError::ConfigParse(err.to_string()))?;
    fs::write(&path, data)?;
    set_owner_only(&path)?;
    Ok(())
}

pub fn model_cache_store_from_config(config: &AikitConfig) -> ModelCacheStore {
    let providers = config
        .providers
        .iter()
        .filter_map(|provider| {
            provider
                .models_cache
                .clone()
                .map(|cache| (provider.id.clone(), cache))
        })
        .collect();
    ModelCacheStore { providers }
}

pub fn state_path(config_path: &Path) -> PathBuf {
    aikit_dir_for_config(config_path).join("state.toml")
}

pub fn model_cache_path(config_path: &Path) -> PathBuf {
    aikit_dir_for_config(config_path)
        .join("cache")
        .join("models.json")
}

pub fn aikit_dir_for_config(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
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
