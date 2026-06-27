use std::path::Path;

use aikit_core::{
    cache::refresh_models,
    config::{
        default_config_path, ActiveSelection, AikitConfig, ApiKeyConfig, ProviderConfig,
        TargetConfig,
    },
    config_ops::{
        add_api_key, add_provider, backup_config_file, delete_api_key, delete_provider,
        update_api_key, update_provider, ApiKeyForm, ProviderForm,
    },
    import::{
        apply_import_candidates, candidate_fingerprint, scan_claude_config, scan_codex_config,
        scan_env, scan_gemini_config, ImportCandidate, ImportPlan,
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
    pub modal_state: ModalState,
    detail_index: usize,
    pub target_statuses: Vec<TargetStatus>,
    import_candidates_for_test: Option<Vec<ImportCandidate>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetStatus {
    pub target_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalState {
    None,
    ProviderForm(ProviderFormState),
    ApiKeyForm(ApiKeyFormState),
    ConfirmDeleteProvider {
        provider_id: String,
    },
    ConfirmDeleteApiKey {
        provider_id: String,
        api_key_id: String,
    },
    ImportPrompt {
        candidates: Vec<ImportCandidate>,
        fingerprint: String,
        selected_indices: Vec<bool>,
        warnings: Vec<String>,
    },
    ImportList {
        candidates: Vec<ImportCandidate>,
        fingerprint: String,
        selected_indices: Vec<bool>,
        cursor: usize,
        warnings: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFormMode {
    Add,
    Edit { original_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFormState {
    pub mode: ProviderFormMode,
    pub current_field: usize,
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: String,
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyFormMode {
    Add,
    Edit { original_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyFormState {
    pub mode: ApiKeyFormMode,
    pub provider_id: String,
    pub current_field: usize,
    pub id: String,
    pub name: String,
    pub value: String,
    pub validation_error: Option<String>,
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
            modal_state: ModalState::None,
            detail_index: 0,
            target_statuses: Vec::new(),
            import_candidates_for_test: None,
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

    pub fn is_modal_open(&self) -> bool {
        !matches!(self.modal_state, ModalState::None)
    }

    pub fn modal_is_confirmation(&self) -> bool {
        matches!(
            self.modal_state,
            ModalState::ConfirmDeleteProvider { .. } | ModalState::ConfirmDeleteApiKey { .. }
        )
    }

    pub fn open_add_provider_modal(&mut self) {
        self.modal_state = ModalState::ProviderForm(ProviderFormState {
            mode: ProviderFormMode::Add,
            current_field: 0,
            id: String::new(),
            name: String::new(),
            base_url: String::new(),
            enabled: "true".into(),
            validation_error: None,
        });
    }

    pub fn open_edit_provider_modal(&mut self) -> Result<()> {
        let provider = self
            .selected_provider()
            .ok_or_else(|| AikitError::ConfigParse("no provider selected".into()))?;
        self.modal_state = ModalState::ProviderForm(ProviderFormState {
            mode: ProviderFormMode::Edit {
                original_id: provider.id.clone(),
            },
            current_field: 0,
            id: provider.id.clone(),
            name: provider.name.clone(),
            base_url: provider.base_url.clone(),
            enabled: provider.enabled.to_string(),
            validation_error: None,
        });
        Ok(())
    }

    pub fn open_add_api_key_modal(&mut self) -> Result<()> {
        let provider_id = self
            .selected_provider()
            .map(|provider| provider.id.clone())
            .ok_or_else(|| AikitError::ConfigParse("no provider selected".into()))?;
        self.modal_state = ModalState::ApiKeyForm(ApiKeyFormState {
            mode: ApiKeyFormMode::Add,
            provider_id,
            current_field: 0,
            id: String::new(),
            name: String::new(),
            value: String::new(),
            validation_error: None,
        });
        Ok(())
    }

    pub fn open_edit_api_key_modal(&mut self) -> Result<()> {
        let provider = self
            .selected_provider()
            .ok_or_else(|| AikitError::ConfigParse("no provider selected".into()))?;
        let key = provider
            .api_keys
            .get(self.key_index)
            .ok_or_else(|| AikitError::ConfigParse("no api key selected".into()))?;
        self.modal_state = ModalState::ApiKeyForm(ApiKeyFormState {
            mode: ApiKeyFormMode::Edit {
                original_id: key.id.clone(),
            },
            provider_id: provider.id.clone(),
            current_field: 0,
            id: key.id.clone(),
            name: key.name.clone(),
            value: key.value.clone(),
            validation_error: None,
        });
        Ok(())
    }

    pub fn open_delete_provider_confirmation(&mut self) -> Result<()> {
        let provider_id = self
            .selected_provider()
            .map(|provider| provider.id.clone())
            .ok_or_else(|| AikitError::ConfigParse("no provider selected".into()))?;
        self.modal_state = ModalState::ConfirmDeleteProvider { provider_id };
        Ok(())
    }

    pub fn open_delete_api_key_confirmation(&mut self) -> Result<()> {
        let provider = self
            .selected_provider()
            .ok_or_else(|| AikitError::ConfigParse("no provider selected".into()))?;
        let api_key_id = provider
            .api_keys
            .get(self.key_index)
            .map(|key| key.id.clone())
            .ok_or_else(|| AikitError::ConfigParse("no api key selected".into()))?;
        self.modal_state = ModalState::ConfirmDeleteApiKey {
            provider_id: provider.id.clone(),
            api_key_id,
        };
        Ok(())
    }

    pub fn scan_import_candidates(&mut self) -> ImportPlan {
        if let Some(candidates) = &self.import_candidates_for_test {
            return ImportPlan {
                candidates: candidates.clone(),
                warnings: Vec::new(),
            };
        }

        let mut plan = scan_env(std::env::vars());
        append_scan_plan(&mut plan, scan_with_default_path("claude", &ClaudeWriter, scan_claude_config));
        append_scan_plan(&mut plan, scan_with_default_path("gemini", &GeminiWriter, scan_gemini_config));
        append_scan_plan(&mut plan, scan_with_default_path("codex", &CodexWriter, scan_codex_config));
        plan
    }

    pub fn open_import_prompt(&mut self) -> Result<()> {
        let plan = self.scan_import_candidates();
        self.open_import_prompt_from_plan(plan)
    }

    pub fn open_import_prompt_from_plan(&mut self, plan: ImportPlan) -> Result<()> {
        if plan.candidates.is_empty() {
            self.set_status("No import candidates found");
            return Ok(());
        }
        let fingerprint = candidate_fingerprint(&plan.candidates);
        self.modal_state = ModalState::ImportPrompt {
            selected_indices: vec![true; plan.candidates.len()],
            candidates: plan.candidates,
            fingerprint,
            warnings: plan.warnings,
        };
        Ok(())
    }

    pub fn open_import_list(&mut self) -> Result<()> {
        if let ModalState::ImportPrompt {
            candidates,
            fingerprint,
            selected_indices,
            warnings,
        } = self.modal_state.clone()
        {
            self.modal_state = ModalState::ImportList {
                candidates,
                fingerprint,
                selected_indices,
                cursor: 0,
                warnings,
            };
            return Ok(());
        }
        Err(AikitError::Provider(
            "import prompt is not open, cannot open list".into(),
        ))
    }

    pub fn confirm_import_all(&mut self) -> Result<()> {
        let (candidates, fingerprint) = match self.modal_state.clone() {
            ModalState::ImportPrompt {
                candidates,
                fingerprint,
                ..
            }
            | ModalState::ImportList {
                candidates,
                fingerprint,
                ..
            } => (candidates, fingerprint),
            _ => {
                return Err(AikitError::Provider(
                    "import prompt is not open, cannot confirm import".into(),
                ))
            }
        };
        self.apply_import_candidates_to_config(candidates, Some(fingerprint))
    }

    pub fn skip_import_prompt(&mut self) -> Result<()> {
        let fingerprint = match self.modal_state.clone() {
            ModalState::ImportPrompt { fingerprint, .. } | ModalState::ImportList { fingerprint, .. } => {
                fingerprint
            }
            _ => {
                return Err(AikitError::Provider(
                    "import prompt is not open, cannot skip import".into(),
                ))
            }
        };
        self.config.import_prompt.skipped_fingerprint = Some(fingerprint);
        self.persist_config_if_file_backed()?;
        self.modal_state = ModalState::None;
        self.set_status("Skipped import prompt");
        Ok(())
    }

    pub fn toggle_import_candidate(&mut self) {
        if let ModalState::ImportList {
            selected_indices,
            cursor,
            ..
        } = &mut self.modal_state
        {
            if *cursor < selected_indices.len() {
                selected_indices[*cursor] = !selected_indices[*cursor];
            }
        }
    }

    pub fn import_list_next(&mut self) {
        if let ModalState::ImportList {
            candidates, cursor, ..
        } = &mut self.modal_state
        {
            if !candidates.is_empty() {
                *cursor = (*cursor + 1) % candidates.len();
            }
        }
    }

    pub fn import_list_previous(&mut self) {
        if let ModalState::ImportList {
            candidates, cursor, ..
        } = &mut self.modal_state
        {
            if !candidates.is_empty() {
                *cursor = (*cursor + candidates.len() - 1) % candidates.len();
            }
        }
    }

    pub fn cancel_import_list(&mut self) -> Result<()> {
        if let ModalState::ImportList {
            candidates,
            fingerprint,
            selected_indices,
            warnings,
            ..
        } = self.modal_state.clone()
        {
            self.modal_state = ModalState::ImportPrompt {
                candidates,
                fingerprint,
                selected_indices,
                warnings,
            };
            return Ok(());
        }
        Err(AikitError::Provider("import list is not open".into()))
    }

    pub fn confirm_selected_imports(&mut self) -> Result<()> {
        let (candidates, selected_indices, fingerprint) = match self.modal_state.clone() {
            ModalState::ImportList {
                candidates,
                selected_indices,
                fingerprint,
                ..
            } => (candidates, selected_indices, fingerprint),
            _ => {
                return Err(AikitError::Provider(
                    "import list is not open, cannot confirm selected imports".into(),
                ))
            }
        };

        let selected = candidates
            .into_iter()
            .zip(selected_indices)
            .filter_map(|(candidate, selected)| selected.then_some(candidate))
            .collect::<Vec<_>>();

        if selected.is_empty() {
            self.set_status("No import candidate selected");
            return Ok(());
        }

        self.apply_import_candidates_to_config(selected, Some(fingerprint))
    }

    pub fn set_import_candidates_for_test(&mut self, candidates: Vec<ImportCandidate>) {
        self.import_candidates_for_test = Some(candidates);
    }

    pub fn set_modal_field(&mut self, field: &str, value: &str) -> Result<()> {
        match &mut self.modal_state {
            ModalState::ProviderForm(form) => {
                form.validation_error = None;
                match field {
                    "id" => form.id = value.into(),
                    "name" => form.name = value.into(),
                    "base_url" => form.base_url = value.into(),
                    "enabled" => form.enabled = value.into(),
                    other => {
                        return Err(AikitError::Provider(format!(
                            "unknown provider field: {other}"
                        )))
                    }
                }
                Ok(())
            }
            ModalState::ApiKeyForm(form) => {
                form.validation_error = None;
                match field {
                    "id" => form.id = value.into(),
                    "name" => form.name = value.into(),
                    "value" => form.value = value.into(),
                    other => {
                        return Err(AikitError::Provider(format!(
                            "unknown api key field: {other}"
                        )))
                    }
                }
                Ok(())
            }
            _ => Err(AikitError::Provider("modal does not have editable fields".into())),
        }
    }

    pub fn modal_next_field(&mut self) {
        match &mut self.modal_state {
            ModalState::ProviderForm(form) => {
                form.current_field = (form.current_field + 1) % 4;
            }
            ModalState::ApiKeyForm(form) => {
                form.current_field = (form.current_field + 1) % 3;
            }
            _ => {}
        }
    }

    pub fn modal_previous_field(&mut self) {
        match &mut self.modal_state {
            ModalState::ProviderForm(form) => {
                form.current_field = (form.current_field + 4 - 1) % 4;
            }
            ModalState::ApiKeyForm(form) => {
                form.current_field = (form.current_field + 3 - 1) % 3;
            }
            _ => {}
        }
    }

    pub fn modal_append_char(&mut self, ch: char) -> Result<()> {
        match &mut self.modal_state {
            ModalState::ProviderForm(form) => {
                form.validation_error = None;
                match form.current_field {
                    0 => form.id.push(ch),
                    1 => form.name.push(ch),
                    2 => form.base_url.push(ch),
                    3 => form.enabled.push(ch),
                    _ => {}
                }
                Ok(())
            }
            ModalState::ApiKeyForm(form) => {
                form.validation_error = None;
                match form.current_field {
                    0 => form.id.push(ch),
                    1 => form.name.push(ch),
                    2 => form.value.push(ch),
                    _ => {}
                }
                Ok(())
            }
            _ => Err(AikitError::Provider("no modal form is open".into())),
        }
    }

    pub fn modal_backspace_field(&mut self) -> Result<()> {
        match &mut self.modal_state {
            ModalState::ProviderForm(form) => {
                form.validation_error = None;
                match form.current_field {
                    0 => {
                        form.id.pop();
                    }
                    1 => {
                        form.name.pop();
                    }
                    2 => {
                        form.base_url.pop();
                    }
                    3 => {
                        form.enabled.pop();
                    }
                    _ => {}
                }
                Ok(())
            }
            ModalState::ApiKeyForm(form) => {
                form.validation_error = None;
                match form.current_field {
                    0 => {
                        form.id.pop();
                    }
                    1 => {
                        form.name.pop();
                    }
                    2 => {
                        form.value.pop();
                    }
                    _ => {}
                }
                Ok(())
            }
            _ => Err(AikitError::Provider("no modal form is open".into())),
        }
    }

    pub fn save_modal(&mut self) -> Result<()> {
        match self.modal_state.clone() {
            ModalState::ProviderForm(form) => self.save_provider_form(form),
            ModalState::ApiKeyForm(form) => self.save_api_key_form(form),
            _ => Err(AikitError::Provider("modal is not a saveable form".into())),
        }
    }

    pub fn confirm_modal(&mut self) -> Result<()> {
        match self.modal_state.clone() {
            ModalState::ConfirmDeleteProvider { provider_id } => {
                let _ = backup_config_file(&self.config_path)?;
                delete_provider(&mut self.config, &provider_id)?;
                self.persist_config_if_file_backed()?;
                self.normalize_selection_indices();
                self.modal_state = ModalState::None;
                self.set_status(format!("Deleted provider {provider_id}"));
                Ok(())
            }
            ModalState::ConfirmDeleteApiKey {
                provider_id,
                api_key_id,
            } => {
                let _ = backup_config_file(&self.config_path)?;
                delete_api_key(&mut self.config, &provider_id, &api_key_id)?;
                self.persist_config_if_file_backed()?;
                self.normalize_selection_indices();
                self.modal_state = ModalState::None;
                self.set_status(format!("Deleted API key {api_key_id}"));
                Ok(())
            }
            _ => Err(AikitError::Provider("modal does not require confirmation".into())),
        }
    }

    pub fn cancel_modal(&mut self) {
        self.modal_state = ModalState::None;
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

    pub fn details_selection_is_api_key(&self) -> bool {
        self.selected_provider()
            .map(|provider| self.detail_index < provider.api_keys.len())
            .unwrap_or(false)
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

    fn save_provider_form(&mut self, form: ProviderFormState) -> Result<()> {
        let enabled = parse_bool_field("enabled", &form.enabled).map_err(|err| {
            self.set_provider_modal_error(err.to_string());
            err
        })?;
        let provider_id = form.id.clone();
        let op_result = match &form.mode {
            ProviderFormMode::Add => add_provider(
                &mut self.config,
                ProviderForm {
                    id: form.id,
                    name: form.name,
                    base_url: form.base_url,
                    enabled,
                },
            ),
            ProviderFormMode::Edit { original_id } => update_provider(
                &mut self.config,
                original_id,
                ProviderForm {
                    id: form.id,
                    name: form.name,
                    base_url: form.base_url,
                    enabled,
                },
            ),
        };

        if let Err(err) = op_result {
            self.set_provider_modal_error(err.to_string());
            return Err(err);
        }

        self.persist_config_if_file_backed()?;
        if let Some(index) = self
            .config
            .providers
            .iter()
            .position(|provider| provider.id == provider_id)
        {
            self.provider_index = index;
        }
        self.normalize_selection_indices();
        self.modal_state = ModalState::None;
        self.set_status("Saved provider");
        Ok(())
    }

    fn save_api_key_form(&mut self, form: ApiKeyFormState) -> Result<()> {
        let key_id = form.id.clone();
        let op_result = match &form.mode {
            ApiKeyFormMode::Add => add_api_key(
                &mut self.config,
                &form.provider_id,
                ApiKeyForm {
                    id: form.id,
                    name: form.name,
                    value: form.value,
                },
            ),
            ApiKeyFormMode::Edit { original_id } => update_api_key(
                &mut self.config,
                &form.provider_id,
                original_id,
                ApiKeyForm {
                    id: form.id,
                    name: form.name,
                    value: form.value,
                },
            ),
        };

        if let Err(err) = op_result {
            self.set_api_key_modal_error(err.to_string());
            return Err(err);
        }

        self.persist_config_if_file_backed()?;
        if let Some(provider_index) = self
            .config
            .providers
            .iter()
            .position(|provider| provider.id == form.provider_id)
        {
            self.provider_index = provider_index;
            if let Some(provider) = self.config.providers.get(provider_index) {
                if let Some(index) = provider.api_keys.iter().position(|key| key.id == key_id) {
                    self.key_index = index;
                }
            }
        }
        self.normalize_selection_indices();
        self.modal_state = ModalState::None;
        self.set_status("Saved API key");
        Ok(())
    }

    fn set_provider_modal_error(&mut self, message: String) {
        if let ModalState::ProviderForm(form) = &mut self.modal_state {
            form.validation_error = Some(message);
        }
    }

    fn set_api_key_modal_error(&mut self, message: String) {
        if let ModalState::ApiKeyForm(form) = &mut self.modal_state {
            form.validation_error = Some(message);
        }
    }

    fn apply_import_candidates_to_config(
        &mut self,
        selected: Vec<ImportCandidate>,
        imported_fingerprint: Option<String>,
    ) -> Result<()> {
        if self.config_path.exists() {
            let _ = backup_config_file(&self.config_path)?;
        }
        let result = apply_import_candidates(&mut self.config, &selected);
        if self
            .config
            .import_prompt
            .skipped_fingerprint
            .as_ref()
            .zip(imported_fingerprint.as_ref())
            .is_some_and(|(skipped, imported)| skipped == imported)
        {
            self.config.import_prompt.skipped_fingerprint = None;
        }
        self.persist_config_if_file_backed()?;
        self.normalize_selection_indices();
        self.modal_state = ModalState::None;
        self.set_status(format!(
            "Imported {} candidate(s), added {} provider(s), {} key(s)",
            selected.len(),
            result.added_providers,
            result.added_keys
        ));
        Ok(())
    }

    fn persist_config_if_file_backed(&self) -> Result<()> {
        // Unit tests may use a dummy single-segment relative path like "config.toml".
        // Skip only when that relative file does not exist yet.
        let is_single_segment_relative = self.config_path.is_relative()
            && self
                .config_path
                .parent()
                .is_none_or(|parent| parent.as_os_str().is_empty());
        if is_single_segment_relative && !self.config_path.exists() {
            return Ok(());
        }
        if is_single_segment_relative {
            return self
                .config
                .save_to(&std::path::PathBuf::from(".").join(&self.config_path));
        }
        self.config.save_to(&self.config_path)
    }
}

fn parse_bool_field(field: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" | "on" => Ok(true),
        "false" | "0" | "no" | "n" | "off" => Ok(false),
        _ => Err(AikitError::Provider(format!(
            "{field} must be a boolean (true/false)"
        ))),
    }
}

fn append_scan_plan(base: &mut ImportPlan, plan: ImportPlan) {
    base.candidates.extend(plan.candidates);
    base.warnings.extend(plan.warnings);
}

fn scan_with_default_path(
    label: &str,
    writer: &dyn TargetWriter,
    scanner: fn(&Path) -> ImportPlan,
) -> ImportPlan {
    match writer.default_path() {
        Ok(path) => scanner(&path),
        Err(err) => ImportPlan {
            candidates: Vec::new(),
            warnings: vec![format!("failed to resolve {label} config path: {err}")],
        },
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
