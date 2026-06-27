use serde::Deserialize;

use crate::{AikitError, Result};

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<ModelItem>,
}

#[derive(Debug, Deserialize)]
struct ModelItem {
    id: String,
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    http: reqwest::Client,
}

impl OpenAiCompatibleClient {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    pub async fn list_models(&self, base_url: &str, api_key: &str) -> Result<Vec<String>> {
        let url = models_url(base_url);
        let response = self
            .http
            .get(url)
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|err| AikitError::Provider(format!("network error: {err}")))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(AikitError::Provider(
                "authentication or permission problem".into(),
            ));
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(AikitError::Provider(
                "models endpoint was not found".into(),
            ));
        }
        if !status.is_success() {
            return Err(AikitError::Provider(format!(
                "provider returned status {status}"
            )));
        }
        let body: ModelListResponse = response
            .json()
            .await
            .map_err(|err| AikitError::Provider(format!("invalid model response: {err}")))?;
        Ok(body.data.into_iter().map(|model| model.id).collect())
    }
}

fn models_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}
