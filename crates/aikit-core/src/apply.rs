use std::path::Path;

use crate::{
    config::{aikit_dir_for_config, load_sidecars, ActiveSelection, AikitConfig, TargetConfig},
    targets::{
        claude::ClaudeWriter, codex::CodexWriter, TargetSelection, TargetWriteResult, TargetWriter,
    },
    AikitError, Result,
};

pub fn load_or_default(config_path: &Path) -> Result<AikitConfig> {
    if config_path.exists() {
        AikitConfig::load_with_sidecars(config_path)
    } else {
        let mut config = AikitConfig::default();
        load_sidecars(config_path, &mut config)?;
        Ok(config)
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
    if active.model_id.trim().is_empty() {
        return Err(AikitError::ConfigParse(format!(
            "active model is empty for provider: {}",
            active.provider_id
        )));
    }

    Ok(TargetSelection {
        base_url: provider.base_url.clone(),
        api_key: api_key.value.clone(),
        model: active.model_id.clone(),
        claude_pin_models: config.claude_pin_models,
        context_1m: config.context_1m,
        bypass_permissions: config.bypass_permissions,
        max_thinking_effort: config.max_thinking_effort,
        claude_disable_betas: config.claude_disable_betas,
    })
}

pub fn write_target(
    target: &TargetConfig,
    selection: &TargetSelection,
    backup_root: &Path,
) -> Result<TargetWriteResult> {
    match target.id.as_str() {
        "claude" => {
            let path = target
                .config_path
                .clone()
                .map(Ok)
                .unwrap_or_else(|| ClaudeWriter.default_path())?;
            ClaudeWriter::write_to_path_with_backup_root(&path, selection, backup_root)
        }
        "codex" => {
            let path = target
                .config_path
                .clone()
                .map(Ok)
                .unwrap_or_else(|| CodexWriter.default_path())?;
            CodexWriter::write_to_path_with_backup_root(&path, selection, backup_root)
        }
        other => Err(AikitError::TargetWrite(format!(
            "unknown target writer: {other}"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TargetResult {
    pub target_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ApplyReport {
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub message: String,
    pub target_results: Vec<TargetResult>,
}

pub fn apply_active_selection(config_path: &Path) -> Result<ApplyReport> {
    let config = load_or_default(config_path)?;
    let selection = active_target_selection(&config)?;
    let mut succeeded = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let mut target_results = Vec::new();

    for target in config.targets.iter().filter(|target| target.enabled) {
        let status = match write_target(target, &selection, &aikit_dir_for_config(config_path)) {
            Ok(_) => {
                succeeded += 1;
                "applied".to_string()
            }
            Err(AikitError::TargetSkipped(msg)) => {
                skipped += 1;
                format!("skipped: {msg}")
            }
            Err(err) => {
                failed += 1;
                format!("failed: {err}")
            }
        };
        target_results.push(TargetResult {
            target_id: target.id.clone(),
            status,
        });
    }

    config.save_with_sidecars(config_path)?;
    Ok(ApplyReport {
        succeeded,
        skipped,
        failed,
        message: format!("Applied {succeeded} target(s), {skipped} skipped, {failed} failed"),
        target_results,
    })
}

pub fn validate_active_selection(config: &AikitConfig, active: &ActiveSelection) -> Result<()> {
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == active.provider_id)
        .ok_or_else(|| {
            AikitError::Provider(format!("provider not found: {}", active.provider_id))
        })?;
    if !provider.enabled {
        return Err(AikitError::Provider(format!(
            "provider is disabled: {}",
            active.provider_id
        )));
    }
    if !provider
        .api_keys
        .iter()
        .any(|key| key.id == active.api_key_id)
    {
        return Err(AikitError::Provider(format!(
            "api key not found: {}",
            active.api_key_id
        )));
    }
    if active.model_id.trim().is_empty() {
        return Err(AikitError::Provider("model id cannot be empty".into()));
    }
    Ok(())
}
