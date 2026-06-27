use std::path::Path;

use aikit_core::{
    cache::refresh_models,
    config::{default_config_path, AikitConfig, TargetConfig},
    provider::OpenAiCompatibleClient,
    targets::{
        claude::ClaudeWriter, codex::CodexWriter, gemini::GeminiWriter, TargetSelection,
        TargetWriteResult, TargetWriter,
    },
    AikitError, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    Providers,
    Details,
    Targets,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    pub focused_pane: FocusedPane,
    pub status: String,
    pub config_path: std::path::PathBuf,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            focused_pane: FocusedPane::Providers,
            status: "Ready".into(),
            config_path: default_config_path().unwrap_or_else(|_| "aikit-config.toml".into()),
        }
    }
}

impl AppState {
    pub fn new(config_path: std::path::PathBuf) -> Self {
        Self {
            config_path,
            ..Self::default()
        }
    }

    pub fn focus_next_pane(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::Providers => FocusedPane::Details,
            FocusedPane::Details => FocusedPane::Targets,
            FocusedPane::Targets => FocusedPane::Providers,
        };
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCommandOutcome {
    pub succeeded: usize,
    pub failed: usize,
    pub message: String,
}

impl AppCommandOutcome {
    fn success(message: impl Into<String>, succeeded: usize, failed: usize) -> Self {
        Self {
            succeeded,
            failed,
            message: message.into(),
        }
    }
}

pub fn active_target_selection(config: &AikitConfig) -> Result<TargetSelection> {
    let active = config
        .active_selection
        .as_ref()
        .ok_or_else(|| AikitError::ConfigParse("no active selection configured".into()))?;
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == active.provider_id)
        .ok_or_else(|| {
            AikitError::ConfigParse(format!("active provider not found: {}", active.provider_id))
        })?;
    if !provider.enabled {
        return Err(AikitError::ConfigParse(format!(
            "active provider is disabled: {}",
            active.provider_id
        )));
    }
    let api_key = provider
        .api_keys
        .iter()
        .find(|key| key.id == active.api_key_id)
        .ok_or_else(|| {
            AikitError::ConfigParse(format!("active api key not found: {}", active.api_key_id))
        })?;
    let cache = provider.models_cache.as_ref().ok_or_else(|| {
        AikitError::ConfigParse(format!(
            "no cached models for active provider: {}",
            active.provider_id
        ))
    })?;
    if !cache.models.iter().any(|model| model == &active.model_id) {
        return Err(AikitError::ConfigParse(format!(
            "active model is not cached: {}",
            active.model_id
        )));
    }

    Ok(TargetSelection {
        base_url: provider.base_url.clone(),
        api_key: api_key.value.clone(),
        model: active.model_id.clone(),
    })
}

pub async fn refresh_active_models(
    config_path: &Path,
    client: &OpenAiCompatibleClient,
) -> Result<AppCommandOutcome> {
    let mut config = load_or_default(config_path)?;
    let active = config
        .active_selection
        .clone()
        .ok_or_else(|| AikitError::ConfigParse("no active selection configured".into()))?;
    let provider = config
        .providers
        .iter_mut()
        .find(|provider| provider.id == active.provider_id)
        .ok_or_else(|| {
            AikitError::ConfigParse(format!("active provider not found: {}", active.provider_id))
        })?;

    let result = refresh_models(provider, &active.api_key_id, client).await;
    let count = provider
        .models_cache
        .as_ref()
        .map(|cache| cache.models.len())
        .unwrap_or(0);
    config.save_to(config_path)?;

    result.map(|_| AppCommandOutcome::success(format!("Refreshed {count} model(s)"), count, 0))
}

pub fn apply_active_selection(config_path: &Path) -> Result<AppCommandOutcome> {
    let config = load_or_default(config_path)?;
    let selection = active_target_selection(&config)?;
    let mut succeeded = 0;
    let mut failed = 0;

    for target in config.targets.iter().filter(|target| target.enabled) {
        match write_target(target, &selection) {
            Ok(_) => succeeded += 1,
            Err(_) => failed += 1,
        }
    }

    config.save_to(config_path)?;
    Ok(AppCommandOutcome::success(
        format!("Applied {succeeded} target(s), {failed} failed"),
        succeeded,
        failed,
    ))
}

fn load_or_default(config_path: &Path) -> Result<AikitConfig> {
    if config_path.exists() {
        AikitConfig::load_from(config_path)
    } else {
        Ok(AikitConfig::default())
    }
}

fn write_target(target: &TargetConfig, selection: &TargetSelection) -> Result<TargetWriteResult> {
    match target.id.as_str() {
        "claude" => match target.config_path.as_deref() {
            Some(path) => ClaudeWriter::write_to_path(path, selection),
            None => ClaudeWriter.write(selection),
        },
        "gemini" => match target.config_path.as_deref() {
            Some(path) => GeminiWriter::write_to_path(path, selection),
            None => GeminiWriter.write(selection),
        },
        "codex" => match target.config_path.as_deref() {
            Some(path) => CodexWriter::write_to_path(path, selection),
            None => CodexWriter.write(selection),
        },
        other => Err(AikitError::TargetWrite(format!(
            "unknown target writer: {other}"
        ))),
    }
}
