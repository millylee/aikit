use aikit_core::{
    cache::refresh_models,
    config::{ApiKeyConfig, ModelCache, ProviderConfig},
    provider::OpenAiCompatibleClient,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn lists_models_from_openai_compatible_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                { "id": "model-a" },
                { "id": "model-b" }
            ]
        })))
        .mount(&server)
        .await;

    let client = OpenAiCompatibleClient::new(reqwest::Client::new());
    let models = client
        .list_models(&format!("{}/v1", server.uri()), "sk-test")
        .await
        .unwrap();

    assert_eq!(models, vec!["model-a", "model-b"]);
}

#[tokio::test]
async fn base_url_without_v1_is_normalized_to_v1_models() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{ "id": "model-a" }]
        })))
        .mount(&server)
        .await;

    let client = OpenAiCompatibleClient::new(reqwest::Client::new());
    let models = client.list_models(&server.uri(), "sk-test").await.unwrap();

    assert_eq!(models, vec!["model-a"]);
}

#[tokio::test]
async fn refresh_failure_keeps_existing_cache() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let mut provider = ProviderConfig {
        id: "p".into(),
        name: "Provider".into(),
        base_url: format!("{}/v1", server.uri()),
        enabled: true,
        api_keys: vec![ApiKeyConfig {
            id: "k".into(),
            name: "Key".into(),
            value: "bad".into(),
        }],
        models_cache: Some(ModelCache {
            refreshed_at: "old".into(),
            models: vec!["old-model".into()],
            last_error: None,
        }),
    };
    let client = OpenAiCompatibleClient::new(reqwest::Client::new());

    let result = refresh_models(&mut provider, "k", &client).await;

    assert!(result.is_err());
    let cache = provider.models_cache.unwrap();
    assert_eq!(cache.models, vec!["old-model"]);
    assert!(cache.last_error.unwrap().contains("authentication"));
}
