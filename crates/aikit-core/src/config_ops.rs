use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    config::{AikitConfig, ApiKeyConfig, ProviderConfig},
    AikitError, Result,
};

pub struct ProviderForm {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
}

pub struct ApiKeyForm {
    pub id: String,
    pub name: String,
    pub value: String,
}

pub fn add_provider(config: &mut AikitConfig, form: ProviderForm) -> Result<()> {
    validate_provider_form(&form)?;
    if config
        .providers
        .iter()
        .any(|provider| provider.id == form.id)
    {
        return Err(AikitError::Provider(format!(
            "provider id already exists: {}",
            form.id
        )));
    }

    config.providers.push(ProviderConfig {
        id: form.id,
        name: form.name,
        base_url: form.base_url,
        enabled: form.enabled,
        api_keys: Vec::new(),
        models_cache: None,
    });
    Ok(())
}

pub fn update_provider(config: &mut AikitConfig, old_id: &str, form: ProviderForm) -> Result<()> {
    validate_provider_form(&form)?;

    let provider_index = config
        .providers
        .iter()
        .position(|provider| provider.id == old_id)
        .ok_or_else(|| AikitError::Provider(format!("provider not found: {old_id}")))?;

    if form.id != old_id
        && config
            .providers
            .iter()
            .any(|provider| provider.id == form.id && provider.id != old_id)
    {
        return Err(AikitError::Provider(format!(
            "provider id already exists: {}",
            form.id
        )));
    }

    let provider = &mut config.providers[provider_index];
    provider.id = form.id.clone();
    provider.name = form.name;
    provider.base_url = form.base_url;
    provider.enabled = form.enabled;

    if old_id != form.id {
        if let Some(active) = config.active_selection.as_mut() {
            if active.provider_id == old_id {
                active.provider_id = form.id;
            }
        }
    }

    Ok(())
}

pub fn delete_provider(config: &mut AikitConfig, provider_id: &str) -> Result<()> {
    let provider_index = config
        .providers
        .iter()
        .position(|provider| provider.id == provider_id)
        .ok_or_else(|| AikitError::Provider(format!("provider not found: {provider_id}")))?;

    config.providers.remove(provider_index);

    if config
        .active_selection
        .as_ref()
        .is_some_and(|active| active.provider_id == provider_id)
    {
        config.active_selection = None;
    }

    Ok(())
}

pub fn add_api_key(config: &mut AikitConfig, provider_id: &str, form: ApiKeyForm) -> Result<()> {
    validate_api_key_form(&form)?;

    let provider = provider_mut(config, provider_id)?;
    if provider.api_keys.iter().any(|key| key.id == form.id) {
        return Err(AikitError::Provider(format!(
            "api key id already exists: {}",
            form.id
        )));
    }

    provider.api_keys.push(ApiKeyConfig {
        id: form.id,
        name: form.name,
        value: form.value,
    });
    Ok(())
}

pub fn update_api_key(
    config: &mut AikitConfig,
    provider_id: &str,
    old_key_id: &str,
    form: ApiKeyForm,
) -> Result<()> {
    validate_api_key_form(&form)?;

    let provider = provider_mut(config, provider_id)?;
    let key_index = provider
        .api_keys
        .iter()
        .position(|key| key.id == old_key_id)
        .ok_or_else(|| AikitError::Provider(format!("api key not found: {old_key_id}")))?;

    if form.id != old_key_id
        && provider
            .api_keys
            .iter()
            .any(|key| key.id == form.id && key.id != old_key_id)
    {
        return Err(AikitError::Provider(format!(
            "api key id already exists: {}",
            form.id
        )));
    }

    let key = &mut provider.api_keys[key_index];
    key.id = form.id.clone();
    key.name = form.name;
    key.value = form.value;

    if old_key_id != form.id {
        if let Some(active) = config.active_selection.as_mut() {
            if active.provider_id == provider_id && active.api_key_id == old_key_id {
                active.api_key_id = form.id;
            }
        }
    }

    Ok(())
}

pub fn delete_api_key(config: &mut AikitConfig, provider_id: &str, key_id: &str) -> Result<()> {
    let provider = provider_mut(config, provider_id)?;
    let key_index = provider
        .api_keys
        .iter()
        .position(|key| key.id == key_id)
        .ok_or_else(|| AikitError::Provider(format!("api key not found: {key_id}")))?;
    provider.api_keys.remove(key_index);

    if config
        .active_selection
        .as_ref()
        .is_some_and(|active| active.provider_id == provider_id && active.api_key_id == key_id)
    {
        config.active_selection = None;
    }

    Ok(())
}

pub fn backup_config_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| AikitError::Provider(format!("system time error: {err}")))?
        .as_millis();

    let backup_file_name = format!(
        "{}.bak.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.toml"),
        timestamp_ms
    );
    let backup_path = path.with_file_name(backup_file_name);
    fs::copy(path, &backup_path)?;
    Ok(Some(backup_path))
}

fn provider_mut<'a>(
    config: &'a mut AikitConfig,
    provider_id: &str,
) -> Result<&'a mut ProviderConfig> {
    config
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AikitError::Provider(format!("provider not found: {provider_id}")))
}

fn validate_provider_form(form: &ProviderForm) -> Result<()> {
    if form.id.trim().is_empty() {
        return Err(AikitError::Provider("provider id cannot be empty".into()));
    }
    if form.name.trim().is_empty() {
        return Err(AikitError::Provider("provider name cannot be empty".into()));
    }
    if form.base_url.trim().is_empty() {
        return Err(AikitError::Provider(
            "provider base url cannot be empty".into(),
        ));
    }
    reqwest::Url::parse(&form.base_url)
        .map_err(|err| AikitError::Provider(format!("invalid provider base url: {err}")))?;
    Ok(())
}

fn validate_api_key_form(form: &ApiKeyForm) -> Result<()> {
    if form.id.trim().is_empty() {
        return Err(AikitError::Provider("api key id cannot be empty".into()));
    }
    if form.name.trim().is_empty() {
        return Err(AikitError::Provider("api key name cannot be empty".into()));
    }
    if form.value.trim().is_empty() {
        return Err(AikitError::Provider("api key value cannot be empty".into()));
    }
    Ok(())
}
