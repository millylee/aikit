use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

use crate::config::{ActiveSelection, AikitConfig, ApiKeyConfig, ProviderConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportSource {
    Env,
    Claude,
    Gemini,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidate {
    pub source: ImportSource,
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: Option<String>,
    pub api_key_name: Option<String>,
    pub api_key_value: Option<String>,
    pub model: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportPlan {
    pub candidates: Vec<ImportCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportResult {
    pub added_providers: usize,
    pub updated_providers: usize,
    pub added_keys: usize,
    pub active_selection_updated: bool,
    pub warnings: Vec<String>,
}

pub fn scan_env(vars: impl IntoIterator<Item = (String, String)>) -> ImportPlan {
    let mut openai_api_key = None;
    let mut openai_base_url = None;
    let mut openai_model = None;

    let mut anthropic_api_key = None;
    let mut anthropic_base_url = None;
    let mut anthropic_model = None;

    let mut gemini_api_key = None;
    let mut gemini_base_url = None;
    let mut gemini_model = None;

    for (name, value) in vars {
        match name.as_str() {
            "OPENAI_API_KEY" => openai_api_key = Some(value),
            "OPENAI_BASE_URL" => openai_base_url = Some(value),
            "OPENAI_MODEL" => openai_model = Some(value),
            "ANTHROPIC_API_KEY" => anthropic_api_key = Some(value),
            "ANTHROPIC_BASE_URL" => anthropic_base_url = Some(value),
            "ANTHROPIC_MODEL" => anthropic_model = Some(value),
            "GEMINI_API_KEY" => gemini_api_key = Some(value),
            "GEMINI_BASE_URL" => gemini_base_url = Some(value),
            "GEMINI_MODEL" => gemini_model = Some(value),
            _ => {}
        }
    }

    let mut candidates = Vec::new();
    push_candidate_if_present(
        &mut candidates,
        "openai",
        "OpenAI",
        openai_api_key,
        openai_base_url,
        openai_model,
    );
    push_candidate_if_present(
        &mut candidates,
        "anthropic",
        "Anthropic",
        anthropic_api_key,
        anthropic_base_url,
        anthropic_model,
    );
    push_candidate_if_present(
        &mut candidates,
        "gemini",
        "Gemini",
        gemini_api_key,
        gemini_base_url,
        gemini_model,
    );

    ImportPlan {
        candidates,
        warnings: Vec::new(),
    }
}

fn push_candidate_if_present(
    candidates: &mut Vec<ImportCandidate>,
    provider_id: &str,
    provider_name: &str,
    api_key_value: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
) {
    if api_key_value.is_none() && base_url.is_none() && model.is_none() {
        return;
    }

    let api_key_name = api_key_value
        .as_ref()
        .map(|_| format!("{}_API_KEY", provider_id.to_uppercase()));

    candidates.push(ImportCandidate {
        source: ImportSource::Env,
        provider_id: provider_id.to_string(),
        provider_name: provider_name.to_string(),
        base_url,
        api_key_name,
        api_key_value,
        model,
        warnings: Vec::new(),
    });
}

pub fn candidate_fingerprint(candidates: &[ImportCandidate]) -> String {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| {
        (
            a.source,
            a.provider_id.as_str(),
            a.base_url.as_deref(),
            a.api_key_name.as_deref(),
            a.api_key_value.as_deref(),
            a.model.as_deref(),
        )
            .cmp(&(
                b.source,
                b.provider_id.as_str(),
                b.base_url.as_deref(),
                b.api_key_name.as_deref(),
                b.api_key_value.as_deref(),
                b.model.as_deref(),
            ))
    });

    let mut joined = String::new();
    for candidate in sorted {
        let source = match candidate.source {
            ImportSource::Env => "env",
            ImportSource::Claude => "claude",
            ImportSource::Gemini => "gemini",
            ImportSource::Codex => "codex",
        };
        joined.push_str(source);
        joined.push('|');
        joined.push_str(&candidate.provider_id);
        joined.push('|');
        joined.push_str(candidate.base_url.as_deref().unwrap_or_default());
        joined.push('|');
        joined.push_str(candidate.api_key_name.as_deref().unwrap_or_default());
        joined.push('|');
        joined.push_str(candidate.api_key_value.as_deref().unwrap_or_default());
        joined.push('|');
        joined.push_str(candidate.model.as_deref().unwrap_or_default());
        joined.push('\n');
    }

    let mut hasher = DefaultHasher::new();
    joined.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub fn scan_claude_config(path: &Path) -> ImportPlan {
    scan_claude_json_config(path)
}

pub fn scan_gemini_config(path: &Path) -> ImportPlan {
    scan_json_aikit_config(path, ImportSource::Gemini)
}

pub fn scan_codex_config(path: &Path) -> ImportPlan {
    let mut warnings = Vec::new();
    let data = match read_file_if_exists(path) {
        Ok(Some(data)) => data,
        Ok(None) => return ImportPlan::default(),
        Err(err) => {
            return ImportPlan {
                candidates: Vec::new(),
                warnings: vec![format!("failed to read codex config: {err}")],
            }
        }
    };

    let value: TomlValue = match toml::from_str(&data) {
        Ok(value) => value,
        Err(err) => {
            return ImportPlan {
                candidates: Vec::new(),
                warnings: vec![format!("failed to parse codex config: {err}")],
            }
        }
    };

    let model = value
        .get("model")
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned);
    let provider_id = value
        .get("model_provider")
        .and_then(TomlValue::as_str)
        .unwrap_or("aikit")
        .to_string();

    let provider_table = value
        .get("model_providers")
        .and_then(TomlValue::as_table)
        .and_then(|providers| providers.get(provider_id.as_str()))
        .and_then(TomlValue::as_table);

    let base_url = provider_table
        .and_then(|table| table.get("base_url"))
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned);
    let env_key_name = provider_table
        .and_then(|table| table.get("env_key"))
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned);

    let env_table = value.get("env").and_then(TomlValue::as_table);

    let env_key_value = if let Some(ref key_name) = env_key_name {
        env_table
            .and_then(|table| table.get(key_name))
            .and_then(TomlValue::as_str)
            .map(ToOwned::to_owned)
    } else {
        None
    };

    let fallback_env_key_value = if env_key_value.is_none() {
        env_table
            .and_then(|table| {
                table
                    .get("AIKIT_API_KEY")
                    .or_else(|| table.get("OPENAI_API_KEY"))
            })
            .and_then(TomlValue::as_str)
            .map(ToOwned::to_owned)
    } else {
        None
    };

    let legacy_api_key_value = provider_table
        .and_then(|table| table.get("api_key"))
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned);
    let auth_path = path
        .parent()
        .map(|parent| parent.join("auth.json"))
        .unwrap_or_else(|| "auth.json".into());
    let auth_api_key_value = match read_file_if_exists(&auth_path) {
        Ok(Some(data)) => match serde_json::from_str::<JsonValue>(&data) {
            Ok(value) => value
                .get("OPENAI_API_KEY")
                .and_then(JsonValue::as_str)
                .map(ToOwned::to_owned),
            Err(err) => {
                warnings.push(format!("failed to parse codex auth config: {err}"));
                None
            }
        },
        Ok(None) => None,
        Err(err) => {
            warnings.push(format!("failed to read codex auth config: {err}"));
            None
        }
    };
    let (api_key_value, api_key_name) =
        if let (Some(val), Some(name)) = (env_key_value, env_key_name) {
            (Some(val), Some(name))
        } else if let Some(val) = fallback_env_key_value {
            let name = if env_table.and_then(|t| t.get("AIKIT_API_KEY")).is_some() {
                "AIKIT_API_KEY".to_string()
            } else {
                "OPENAI_API_KEY".to_string()
            };
            (Some(val), Some(name))
        } else if let Some(val) = auth_api_key_value {
            (Some(val), Some("OPENAI_API_KEY".to_string()))
        } else if let Some(val) = legacy_api_key_value {
            (Some(val), Some("codex-api-key".to_string()))
        } else {
            (None, None)
        };

    if base_url.is_none() && api_key_value.is_none() && model.is_none() {
        return ImportPlan {
            candidates: Vec::new(),
            warnings,
        };
    }

    ImportPlan {
        candidates: vec![ImportCandidate {
            source: ImportSource::Codex,
            provider_id: provider_id.clone(),
            provider_name: title_case_provider_name(&provider_id),
            base_url,
            api_key_name,
            api_key_value,
            model,
            warnings: Vec::new(),
        }],
        warnings,
    }
}

pub fn apply_import_candidates(
    config: &mut AikitConfig,
    selected: &[ImportCandidate],
) -> ImportResult {
    let mut result = ImportResult::default();

    for candidate in selected {
        if !candidate.warnings.is_empty() {
            result.warnings.extend(candidate.warnings.clone());
        }

        let provider_idx = find_provider_index(config, candidate);
        let provider_idx = match provider_idx {
            Some(idx) => {
                result.updated_providers += 1;
                idx
            }
            None => {
                let Some(base_url) = candidate.base_url.clone() else {
                    result.warnings.push(format!(
                        "Skipped import candidate '{}' from {:?}: base URL is required before import",
                        candidate.provider_name, candidate.source
                    ));
                    continue;
                };
                config.providers.push(ProviderConfig {
                    id: candidate.provider_id.clone(),
                    name: candidate.provider_name.clone(),
                    base_url,
                    enabled: true,
                    api_keys: Vec::new(),
                    manual_models: Vec::new(),
                    models_cache: None,
                });
                result.added_providers += 1;
                config.providers.len() - 1
            }
        };

        let provider = &mut config.providers[provider_idx];
        if provider.base_url.is_empty() {
            if let Some(base_url) = &candidate.base_url {
                provider.base_url = base_url.clone();
            }
        }

        if let Some(model) = &candidate.model {
            let cached = provider
                .models_cache
                .as_ref()
                .is_some_and(|cache| cache.models.iter().any(|cached| cached == model));
            if !cached && !provider.manual_models.iter().any(|manual| manual == model) {
                provider.manual_models.push(model.clone());
            }
        }

        let mut selected_key_id = provider.api_keys.first().map(|key| key.id.clone());

        if let Some(api_key_value) = &candidate.api_key_value {
            let base_key_id = normalize_api_key_id(candidate.api_key_name.as_deref());
            let has_same_id = provider.api_keys.iter().any(|key| key.id == base_key_id);
            let has_same_value = provider
                .api_keys
                .iter()
                .any(|key| key.value == *api_key_value);

            if !has_same_id && !has_same_value {
                let key_name = candidate
                    .api_key_name
                    .clone()
                    .unwrap_or_else(|| "Imported".to_string());
                provider.api_keys.push(ApiKeyConfig {
                    id: base_key_id.clone(),
                    name: key_name,
                    value: api_key_value.clone(),
                });
                result.added_keys += 1;
                selected_key_id = Some(base_key_id);
            } else if has_same_id {
                selected_key_id = Some(base_key_id);
            } else if let Some(existing) = provider
                .api_keys
                .iter()
                .find(|key| key.value == *api_key_value)
                .map(|key| key.id.clone())
            {
                selected_key_id = Some(existing);
            }
        }

        if config.active_selection.is_none() {
            if let (Some(model), Some(api_key_id)) = (&candidate.model, selected_key_id) {
                config.active_selection = Some(ActiveSelection {
                    provider_id: provider.id.clone(),
                    api_key_id,
                    model_id: model.clone(),
                });
                result.active_selection_updated = true;
            }
        }
    }

    result
}

fn scan_json_aikit_config(path: &Path, source: ImportSource) -> ImportPlan {
    let data = match read_file_if_exists(path) {
        Ok(Some(data)) => data,
        Ok(None) => return ImportPlan::default(),
        Err(err) => {
            return ImportPlan {
                candidates: Vec::new(),
                warnings: vec![format!("failed to read config: {err}")],
            }
        }
    };

    let value: JsonValue = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(err) => {
            return ImportPlan {
                candidates: Vec::new(),
                warnings: vec![format!("failed to parse json config: {err}")],
            }
        }
    };

    let aikit = value.get("aikit").and_then(JsonValue::as_object);
    let base_url = aikit
        .and_then(|node| node.get("base_url"))
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let api_key_value = aikit
        .and_then(|node| node.get("api_key"))
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let model = aikit
        .and_then(|node| node.get("model"))
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);

    if base_url.is_none() && api_key_value.is_none() && model.is_none() {
        return ImportPlan::default();
    }

    let provider_id = "aikit".to_string();
    ImportPlan {
        candidates: vec![ImportCandidate {
            source,
            provider_id: provider_id.clone(),
            provider_name: title_case_provider_name(&provider_id),
            base_url,
            api_key_name: api_key_value.as_ref().map(|_| "aikit-api-key".to_string()),
            api_key_value,
            model,
            warnings: Vec::new(),
        }],
        warnings: Vec::new(),
    }
}

fn scan_claude_json_config(path: &Path) -> ImportPlan {
    let data = match read_file_if_exists(path) {
        Ok(Some(data)) => data,
        Ok(None) => return ImportPlan::default(),
        Err(err) => {
            return ImportPlan {
                candidates: Vec::new(),
                warnings: vec![format!("failed to read claude config: {err}")],
            }
        }
    };

    let value: JsonValue = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(err) => {
            return ImportPlan {
                candidates: Vec::new(),
                warnings: vec![format!("failed to parse claude config: {err}")],
            }
        }
    };

    let env = value.get("env").and_then(JsonValue::as_object);
    let legacy_aikit = value.get("aikit").and_then(JsonValue::as_object);
    let base_url = env
        .and_then(|node| node.get("ANTHROPIC_BASE_URL"))
        .or_else(|| legacy_aikit.and_then(|node| node.get("base_url")))
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let api_key_value = env
        .and_then(|node| node.get("ANTHROPIC_AUTH_TOKEN"))
        .or_else(|| env.and_then(|node| node.get("ANTHROPIC_API_KEY")))
        .or_else(|| legacy_aikit.and_then(|node| node.get("api_key")))
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let model = value
        .get("model")
        .and_then(JsonValue::as_str)
        .or_else(|| {
            env.and_then(|node| node.get("ANTHROPIC_MODEL"))
                .and_then(JsonValue::as_str)
        })
        .or_else(|| {
            legacy_aikit
                .and_then(|node| node.get("model"))
                .and_then(JsonValue::as_str)
        })
        .map(ToOwned::to_owned);

    if base_url.is_none() && api_key_value.is_none() && model.is_none() {
        return ImportPlan::default();
    }

    ImportPlan {
        candidates: vec![ImportCandidate {
            source: ImportSource::Claude,
            provider_id: "claude".to_string(),
            provider_name: "Claude".to_string(),
            base_url,
            api_key_name: api_key_value
                .as_ref()
                .map(|_| "ANTHROPIC_AUTH_TOKEN".to_string()),
            api_key_value,
            model,
            warnings: Vec::new(),
        }],
        warnings: Vec::new(),
    }
}

fn find_provider_index(config: &AikitConfig, candidate: &ImportCandidate) -> Option<usize> {
    if let Some(base_url) = &candidate.base_url {
        if let Some(idx) = config
            .providers
            .iter()
            .position(|provider| provider.base_url == *base_url)
        {
            return Some(idx);
        }
    }

    config
        .providers
        .iter()
        .position(|provider| provider.id == candidate.provider_id)
}

fn normalize_api_key_id(api_key_name: Option<&str>) -> String {
    let mut normalized = String::new();
    let mut last_dash = false;

    for ch in api_key_name.unwrap_or("imported").chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !normalized.is_empty() {
            normalized.push('-');
            last_dash = true;
        }
    }

    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "imported".to_string()
    } else {
        normalized
    }
}

fn title_case_provider_name(provider_id: &str) -> String {
    let mut chars = provider_id.chars();
    if let Some(first) = chars.next() {
        format!(
            "{}{}",
            first.to_ascii_uppercase(),
            chars.as_str().to_ascii_lowercase()
        )
    } else {
        "Imported".to_string()
    }
}

fn read_file_if_exists(path: &Path) -> Result<Option<String>, std::io::Error> {
    match fs::read_to_string(path) {
        Ok(data) => Ok(Some(data)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}
