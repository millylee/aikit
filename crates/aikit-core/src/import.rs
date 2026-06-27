use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
