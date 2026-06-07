//! Shared OpenRouter/Ollama model catalog transport.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use super::model_catalog_dtos::{
    ModelsDevCatalog, OllamaTagModel, OllamaTagsResponse, OrModelsResponse,
};
use super::provider_config::{
    is_ollama_cloud_base_url, ollama_tags_url, ProviderConfigError, ProviderSettings,
    OPENROUTER_BASE_URL, REQUEST_TIMEOUT,
};
use crate::store::PodcastStore;

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const MODELS_DEV_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Serialize)]
pub struct ProviderModelCatalog {
    pub models: Vec<ProviderModelOption>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ProviderModelOption {
    pub provider: &'static str,
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub provider_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cost_per_million: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_cost_per_million: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_cost_per_million: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_cost_per_million: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<i64>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokenizer: Option<String>,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
    pub supports_response_format: bool,
    pub supports_structured_outputs: bool,
    pub open_weights: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_moderated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at_epoch_secs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_cutoff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    pub search_text: String,
}

impl ProviderModelOption {
    fn is_text_output(&self) -> bool {
        self.output_modalities.is_empty()
            || self.output_modalities.iter().any(|item| item == "text")
    }

    fn is_compatible(&self) -> bool {
        self.is_text_output() && self.supports_response_format
    }
}

#[derive(Debug)]
pub enum ModelCatalogError {
    Config(ProviderConfigError),
    Transport(String),
    ProviderStatus(u16, String),
    Decode(String),
}

impl std::fmt::Display for ModelCatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(ProviderConfigError::StoreUnavailable) => {
                write!(f, "settings store unavailable")
            }
            Self::Transport(message) => write!(f, "provider catalog transport failed: {message}"),
            Self::ProviderStatus(status, body) => {
                write!(
                    f,
                    "provider catalog returned HTTP {status}: {}",
                    body.chars().take(300).collect::<String>()
                )
            }
            Self::Decode(message) => write!(f, "provider catalog decode failed: {message}"),
        }
    }
}

impl From<ProviderConfigError> for ModelCatalogError {
    fn from(error: ProviderConfigError) -> Self {
        Self::Config(error)
    }
}

pub async fn fetch_model_catalog(
    store: Arc<Mutex<PodcastStore>>,
) -> Result<ProviderModelCatalog, ModelCatalogError> {
    let settings = ProviderSettings::from_store(&store)?;
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| ModelCatalogError::Transport(e.to_string()))?;

    let openrouter = fetch_openrouter_models(&client);
    let models_dev = fetch_models_dev_catalog(&client);
    let ollama = fetch_ollama_models(&client, &settings);
    let (openrouter, models_dev, ollama) = tokio::join!(openrouter, models_dev, ollama);

    let metadata = models_dev.ok();
    let mut errors = Vec::new();
    let mut models = match openrouter {
        Ok(models) => models
            .into_iter()
            .map(|model| model.into_option(metadata.as_ref()))
            .collect::<Vec<_>>(),
        Err(error) => {
            errors.push(error.to_string());
            Vec::new()
        }
    };
    match ollama {
        Ok(ollama) => {
            models.extend(ollama.into_iter().map(OllamaTagModel::into_option));
        }
        Err(error) => errors.push(error.to_string()),
    }
    if models.is_empty() && !errors.is_empty() {
        return Err(ModelCatalogError::Transport(errors.join("; ")));
    }
    sort_options(&mut models);
    Ok(ProviderModelCatalog { models })
}

async fn fetch_openrouter_models(
    client: &reqwest::Client,
) -> Result<Vec<super::model_catalog_dtos::OrModel>, ModelCatalogError> {
    let response: OrModelsResponse = get_json(
        client,
        &format!("{OPENROUTER_BASE_URL}/models"),
        None,
        REQUEST_TIMEOUT,
    )
    .await?;
    Ok(response.data)
}

async fn fetch_models_dev_catalog(
    client: &reqwest::Client,
) -> Result<ModelsDevCatalog, ModelCatalogError> {
    let providers = get_json(client, MODELS_DEV_URL, None, MODELS_DEV_TIMEOUT).await?;
    Ok(ModelsDevCatalog { providers })
}

async fn fetch_ollama_models(
    client: &reqwest::Client,
    settings: &ProviderSettings,
) -> Result<Vec<OllamaTagModel>, ModelCatalogError> {
    let api_key = settings
        .ollama_key
        .clone()
        .filter(|key| !key.trim().is_empty());
    if is_ollama_cloud_base_url(&settings.ollama_base_url) && api_key.is_none() {
        return Ok(Vec::new());
    }
    let response: OllamaTagsResponse = get_json(
        client,
        &ollama_tags_url(&settings.ollama_base_url),
        api_key,
        REQUEST_TIMEOUT,
    )
    .await?;
    Ok(response.models)
}

async fn get_json<T: for<'de> serde::Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    api_key: Option<String>,
    timeout: Duration,
) -> Result<T, ModelCatalogError> {
    let mut request = client.get(url).timeout(timeout);
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    if url.starts_with(OPENROUTER_BASE_URL) {
        request = request.header("X-Title", "Podcastr");
    }
    let response = request
        .send()
        .await
        .map_err(|e| ModelCatalogError::Transport(e.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| ModelCatalogError::Transport(e.to_string()))?;
    if !status.is_success() {
        return Err(ModelCatalogError::ProviderStatus(status.as_u16(), text));
    }
    serde_json::from_str(&text).map_err(|e| ModelCatalogError::Decode(e.to_string()))
}

fn sort_options(models: &mut [ProviderModelOption]) {
    models.sort_by(|lhs, rhs| {
        rhs.is_compatible()
            .cmp(&lhs.is_compatible())
            .then_with(|| provider_rank(lhs.provider).cmp(&provider_rank(rhs.provider)))
            .then_with(|| rhs.created_at_epoch_secs.cmp(&lhs.created_at_epoch_secs))
            .then_with(|| lhs.name.to_lowercase().cmp(&rhs.name.to_lowercase()))
    });
}

fn provider_rank(provider: &str) -> u8 {
    match provider {
        "openrouter" => 0,
        "ollama" => 1,
        _ => 2,
    }
}
