use std::path::Path;

use aikit_core::{
    cache::refresh_models,
    config::{
        default_config_path, ActiveSelection, AikitConfig, ApiKeyConfig, ProviderConfig,
        TargetConfig,
    },
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
    pub config: AikitConfig,
    pub provider_index: usize,
    pub key_index: usize,
    pub model_index: usize,
    pub target_index: usize,
    detail_index: usize,
    pub target_statuses: Vec<TargetStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetStatus {
    pub target_id: String,
    pub message: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            focused_pane: FocusedPane::Providers,
            status: "Ready".into(),
            config_path: default_config_path().unwrap_or_else(|_| "aikit-config.toml".into()),
            config: AikitConfig::default(),
            provider_index: 0,
            key_index: 0,
            model_index: 0,
            target_index: 0,
            detail_index: 0,
            target_statuses: Vec::new(),
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

    pub fn from_config(config_path: std::path::PathBuf, config: AikitConfig) -> Self {
        let mut state = Self {
            config_path,
            config,
            ..Self::default()
        };
        state.normalize_selection_indices();
        state
    }

    pub fn load_config(&mut self) -> Result<()> {
        self.config = load_or_default(&self.config_path)?;
        self.normalize_selection_indices();
        self.set_status(format!(
            "Loaded {} provider(s), {} target(s)",
            self.config.providers.len(),
            self.config.targets.len()
        ));
        Ok(())
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

    pub fn selected_provider(&self) -> Option<&ProviderConfig> {
        self.config.providers.get(self.provider_index)
    }

    pub fn selected_key(&self) -> Option<&ApiKeyConfig> {
        self.selected_provider()
            .and_then(|provider| provider.api_keys.get(self.key_index))
    }

    pub fn selected_model(&self) -> Option<&str> {
        self.selected_provider()
            .and_then(|provider| provider.models_cache.as_ref())
            .and_then(|cache| cache.models.get(self.model_index))
            .map(String::as_str)
    }

    pub fn selected_target(&self) -> Option<&TargetConfig> {
        self.config.targets.get(self.target_index)
    }

    pub fn target_status(&self, target_id: &str) -> Option<&str> {
        self.target_statuses
            .iter()
            .find(|status| status.target_id == target_id)
            .map(|status| status.message.as_str())
    }

    pub fn select_next(&mut self) {
        match self.focused_pane {
            FocusedPane::Providers => {
                if !self.config.providers.is_empty() {
                    self.provider_index = (self.provider_index + 1) % self.config.providers.len();
                    self.sync_provider_children();
                }
            }
            FocusedPane::Details => {
                let count = self.detail_item_count();
                if count > 0 {
                    self.detail_index = (self.detail_index + 1) % count;
                    self.apply_detail_index();
                }
            }
            FocusedPane::Targets => {
                if !self.config.targets.is_empty() {
                    self.target_index = (self.target_index + 1) % self.config.targets.len();
                }
            }
        }
    }

    pub fn select_previous(&mut self) {
        match self.focused_pane {
            FocusedPane::Providers => {
                if !self.config.providers.is_empty() {
                    self.provider_index = (self.provider_index + self.config.providers.len() - 1)
                        % self.config.providers.len();
                    self.sync_provider_children();
                }
            }
            FocusedPane::Details => {
                let count = self.detail_item_count();
                if count > 0 {
                    self.detail_index = (self.detail_index + count - 1) % count;
                    self.apply_detail_index();
                }
            }
            FocusedPane::Targets => {
                if !self.config.targets.is_empty() {
                    self.target_index = (self.target_index + self.config.targets.len() - 1)
                        % self.config.targets.len();
                }
            }
        }
    }

    pub fn activate_selected(&mut self) {
        match self.focused_pane {
            FocusedPane::Providers | FocusedPane::Details => self.activate_current_selection(),
            FocusedPane::Targets => self.toggle_selected_target(),
        }
    }

    pub fn toggle_selected_target(&mut self) {
        if let Some(target) = self.config.targets.get_mut(self.target_index) {
            target.enabled = !target.enabled;
            let status = if target.enabled {
                "enabled"
            } else {
                "disabled"
            };
            let target_id = target.id.clone();
            self.set_target_status(target_id.clone(), format!("{target_id} {status}"));
        }
    }

    pub async fn refresh_active_models(
        &mut self,
        client: &OpenAiCompatibleClient,
    ) -> Result<AppCommandOutcome> {
        let provider_id = self
            .selected_provider()
            .map(|provider| provider.id.clone())
            .ok_or_else(|| AikitError::ConfigParse("no provider selected".into()))?;
        let api_key_id = self
            .selected_key()
            .map(|key| key.id.clone())
            .ok_or_else(|| AikitError::ConfigParse("no api key selected".into()))?;

        self.config.save_to(&self.config_path)?;
        let outcome =
            refresh_selected_models(&self.config_path, &provider_id, &api_key_id, client).await?;
        self.load_config()?;
        self.set_status(outcome.message.clone());
        Ok(outcome)
    }

    pub fn apply_active_selection(&mut self) -> Result<AppCommandOutcome> {
        self.config.save_to(&self.config_path)?;
        let outcome = apply_active_selection(&self.config_path)?;
        self.target_statuses = outcome.target_statuses.clone();
        self.set_status(outcome.message.clone());
        Ok(outcome)
    }

    pub fn detail_item_count(&self) -> usize {
        self.selected_provider()
            .map(|provider| {
                provider.api_keys.len()
                    + provider
                        .models_cache
                        .as_ref()
                        .map(|cache| cache.models.len())
                        .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    pub fn detail_index(&self) -> usize {
        self.detail_index
    }

    fn normalize_selection_indices(&mut self) {
        self.provider_index = self
            .provider_index
            .min(self.config.providers.len().saturating_sub(1));
        self.target_index = self
            .target_index
            .min(self.config.targets.len().saturating_sub(1));

        if let Some(active) = &self.config.active_selection {
            if let Some(provider_index) = self
                .config
                .providers
                .iter()
                .position(|provider| provider.id == active.provider_id)
            {
                self.provider_index = provider_index;
            }
        }
        self.sync_provider_children();
    }

    fn sync_provider_children(&mut self) {
        let Some(provider) = self.config.providers.get(self.provider_index) else {
            self.key_index = 0;
            self.model_index = 0;
            self.detail_index = 0;
            return;
        };
        let key_count = provider.api_keys.len();
        let model_count = provider
            .models_cache
            .as_ref()
            .map(|cache| cache.models.len())
            .unwrap_or(0);
        let active_key_index = self.config.active_selection.as_ref().and_then(|active| {
            (active.provider_id == provider.id).then(|| {
                provider
                    .api_keys
                    .iter()
                    .position(|key| key.id == active.api_key_id)
            })?
        });
        let active_model_index = self.config.active_selection.as_ref().and_then(|active| {
            (active.provider_id == provider.id).then(|| {
                provider.models_cache.as_ref().and_then(|cache| {
                    cache
                        .models
                        .iter()
                        .position(|model| model == &active.model_id)
                })
            })?
        });

        self.key_index = self.key_index.min(key_count.saturating_sub(1));
        self.model_index = self.model_index.min(model_count.saturating_sub(1));

        if let Some(index) = active_key_index {
            self.key_index = index;
        }
        if let Some(index) = active_model_index {
            self.model_index = index;
        }
        self.detail_index = self
            .detail_index
            .min(self.detail_item_count().saturating_sub(1));
        self.apply_detail_index();
    }

    fn apply_detail_index(&mut self) {
        let Some(provider) = self.selected_provider() else {
            return;
        };
        let key_count = provider.api_keys.len();
        if self.detail_index < key_count {
            self.key_index = self.detail_index;
        } else {
            self.model_index = self.detail_index - key_count;
        }
    }

    fn activate_current_selection(&mut self) {
        let Some(provider) = self.selected_provider() else {
            self.set_status("No provider selected");
            return;
        };
        let provider_id = provider.id.clone();
        let provider_name = provider.name.clone();
        let Some(api_key) = provider.api_keys.get(self.key_index) else {
            self.set_status("Selected provider has no API keys");
            return;
        };
        let api_key_id = api_key.id.clone();
        let api_key_name = api_key.name.clone();
        let Some(model) = provider
            .models_cache
            .as_ref()
            .and_then(|cache| cache.models.get(self.model_index))
        else {
            self.set_status("Selected provider has no cached models");
            return;
        };
        let model = model.clone();

        self.config.active_selection = Some(ActiveSelection {
            provider_id,
            api_key_id,
            model_id: model.clone(),
        });
        self.set_status(format!(
            "Selected {} / {} / {}",
            provider_name, api_key_name, model
        ));
    }

    fn set_target_status(&mut self, target_id: String, message: String) {
        if let Some(status) = self
            .target_statuses
            .iter_mut()
            .find(|status| status.target_id == target_id)
        {
            status.message = message;
        } else {
            self.target_statuses
                .push(TargetStatus { target_id, message });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCommandOutcome {
    pub succeeded: usize,
    pub failed: usize,
    pub message: String,
    pub target_statuses: Vec<TargetStatus>,
}

impl AppCommandOutcome {
    fn success(message: impl Into<String>, succeeded: usize, failed: usize) -> Self {
        Self {
            succeeded,
            failed,
            message: message.into(),
            target_statuses: Vec::new(),
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
    let config = load_or_default(config_path)?;
    let active = config
        .active_selection
        .as_ref()
        .ok_or_else(|| AikitError::ConfigParse("no active selection configured".into()))?;

    refresh_selected_models(config_path, &active.provider_id, &active.api_key_id, client).await
}

pub async fn refresh_selected_models(
    config_path: &Path,
    provider_id: &str,
    api_key_id: &str,
    client: &OpenAiCompatibleClient,
) -> Result<AppCommandOutcome> {
    let mut config = load_or_default(config_path)?;
    let provider = config
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| {
            AikitError::ConfigParse(format!("selected provider not found: {provider_id}"))
        })?;

    let result = refresh_models(provider, api_key_id, client).await;
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
    let mut target_statuses = Vec::new();

    for target in config.targets.iter().filter(|target| target.enabled) {
        match write_target(target, &selection) {
            Ok(_) => {
                succeeded += 1;
                target_statuses.push(TargetStatus {
                    target_id: target.id.clone(),
                    message: "applied".into(),
                });
            }
            Err(err) => {
                failed += 1;
                target_statuses.push(TargetStatus {
                    target_id: target.id.clone(),
                    message: format!("failed: {err}"),
                });
            }
        }
    }

    config.save_to(config_path)?;
    let mut outcome = AppCommandOutcome::success(
        format!("Applied {succeeded} target(s), {failed} failed"),
        succeeded,
        failed,
    );
    outcome.target_statuses = target_statuses;
    Ok(outcome)
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
