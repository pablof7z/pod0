//! Shared provider HTTP transport for shell-initiated LLM calls.
//!
//! Platform shells pass provider/model/prompt intent through FFI. This module
//! owns provider URLs, headers, body shapes, credentials, and response decoding.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::provider_config::{
    is_ollama_cloud_base_url, ollama_chat_url, ollama_embed_url, strip_provider_prefix,
    ProviderConfigError, ProviderSettings, OPENROUTER_BASE_URL, REQUEST_TIMEOUT,
};
use crate::store::PodcastStore;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum ProviderKind {
    #[serde(rename = "openrouter", alias = "open_router")]
    OpenRouter,
    #[serde(rename = "ollama")]
    Ollama,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::OpenRouter => "openrouter",
            ProviderKind::Ollama => "ollama",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CompletionIntent {
    pub provider: ProviderKind,
    pub model: String,
    pub system: String,
    pub user: String,
    #[serde(default)]
    pub response_format: ResponseFormat,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormat {
    Text,
    JsonObject,
}

impl Default for ResponseFormat {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingIntent {
    pub provider: ProviderKind,
    pub model: String,
    pub input: Vec<String>,
    pub dimensions: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct CompletionResult {
    pub text: String,
    pub provider: &'static str,
    pub model: String,
    pub latency_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingResult {
    pub embeddings: Vec<Vec<f32>>,
    pub provider: &'static str,
    pub model: String,
    pub latency_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    pub prompt_tokens: u64,
}

#[derive(Debug)]
pub enum ProviderTransportError {
    MissingCredential(&'static str),
    Transport(String),
    ProviderStatus(u16, String),
    Decode(String),
    Malformed(String),
    StoreUnavailable,
}

impl std::fmt::Display for ProviderTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredential(provider) => write!(f, "{provider} API key is not configured"),
            Self::Transport(message) => write!(f, "provider transport failed: {message}"),
            Self::ProviderStatus(status, body) => {
                write!(
                    f,
                    "provider returned HTTP {status}: {}",
                    body.chars().take(300).collect::<String>()
                )
            }
            Self::Decode(message) => write!(f, "provider response decode failed: {message}"),
            Self::Malformed(message) => write!(f, "provider response malformed: {message}"),
            Self::StoreUnavailable => write!(f, "settings store unavailable"),
        }
    }
}

impl From<ProviderConfigError> for ProviderTransportError {
    fn from(error: ProviderConfigError) -> Self {
        match error {
            ProviderConfigError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

pub async fn complete(
    store: Arc<Mutex<PodcastStore>>,
    intent: CompletionIntent,
) -> Result<CompletionResult, ProviderTransportError> {
    let settings = ProviderSettings::from_store(&store)?;
    match intent.provider {
        ProviderKind::OpenRouter => complete_openrouter(intent, settings).await,
        ProviderKind::Ollama => complete_ollama(intent, settings).await,
    }
}

pub async fn embed(
    store: Arc<Mutex<PodcastStore>>,
    intent: EmbeddingIntent,
) -> Result<EmbeddingResult, ProviderTransportError> {
    let settings = ProviderSettings::from_store(&store)?;
    match intent.provider {
        ProviderKind::OpenRouter => embed_openrouter(intent, settings).await,
        ProviderKind::Ollama => embed_ollama(intent, settings).await,
    }
}

async fn complete_openrouter(
    intent: CompletionIntent,
    settings: ProviderSettings,
) -> Result<CompletionResult, ProviderTransportError> {
    let api_key = settings
        .openrouter_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(ProviderTransportError::MissingCredential("OpenRouter"))?;
    let mut body = json!({
        "model": strip_provider_prefix(&intent.model, "openrouter"),
        "messages": [
            {"role": "system", "content": intent.system},
            {"role": "user", "content": intent.user}
        ],
        "stream": false
    });
    if intent.response_format == ResponseFormat::JsonObject {
        body["response_format"] = json!({"type": "json_object"});
    }
    let started = Instant::now();
    let response = post_json(
        &format!("{OPENROUTER_BASE_URL}/chat/completions"),
        Some(api_key),
        body,
    )
    .await?;
    let text = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderTransportError::Malformed("missing assistant content".to_owned()))?;
    let usage = response.get("usage").cloned();
    let model = response
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_else(|| strip_provider_prefix(&intent.model, "openrouter"))
        .to_owned();
    Ok(CompletionResult {
        text: text.to_owned(),
        provider: ProviderKind::OpenRouter.label(),
        model,
        latency_ms: started.elapsed().as_millis(),
        prompt_tokens: usage_token(&usage, "prompt_tokens"),
        completion_tokens: usage_token(&usage, "completion_tokens"),
        usage,
    })
}

async fn complete_ollama(
    intent: CompletionIntent,
    settings: ProviderSettings,
) -> Result<CompletionResult, ProviderTransportError> {
    let body = json!({
        "model": strip_provider_prefix(&intent.model, "ollama"),
        "messages": [
            {"role": "system", "content": intent.system},
            {"role": "user", "content": intent.user}
        ],
        "stream": false,
        "think": false
    });
    let mut body = body;
    if intent.response_format == ResponseFormat::JsonObject {
        body["format"] = json!("json");
    }
    let api_key = settings.ollama_key.filter(|key| !key.trim().is_empty());
    if is_ollama_cloud_base_url(&settings.ollama_base_url) && api_key.is_none() {
        return Err(ProviderTransportError::MissingCredential("Ollama"));
    }
    let started = Instant::now();
    let response = post_json(&ollama_chat_url(&settings.ollama_base_url), api_key, body).await?;
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(ProviderTransportError::Malformed(error.to_owned()));
    }
    let text = response
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProviderTransportError::Malformed("missing Ollama message content".to_owned())
        })?;
    Ok(CompletionResult {
        text: text.to_owned(),
        provider: ProviderKind::Ollama.label(),
        model: response
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_else(|| strip_provider_prefix(&intent.model, "ollama"))
            .to_owned(),
        latency_ms: started.elapsed().as_millis(),
        usage: None,
        prompt_tokens: response
            .get("prompt_eval_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        completion_tokens: response
            .get("eval_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

async fn embed_openrouter(
    intent: EmbeddingIntent,
    settings: ProviderSettings,
) -> Result<EmbeddingResult, ProviderTransportError> {
    let api_key = settings
        .openrouter_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(ProviderTransportError::MissingCredential("OpenRouter"))?;
    let body = json!({
        "model": strip_provider_prefix(&intent.model, "openrouter"),
        "input": intent.input,
        "dimensions": intent.dimensions
    });
    let started = Instant::now();
    let response = post_json(
        &format!("{OPENROUTER_BASE_URL}/embeddings"),
        Some(api_key),
        body,
    )
    .await?;
    let mut items = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderTransportError::Malformed("missing embeddings data".to_owned()))?
        .clone();
    items.sort_by_key(|item| {
        item.get("index")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX)
    });
    let embeddings = items
        .iter()
        .map(|item| decode_embedding(item.get("embedding")))
        .collect::<Result<Vec<_>, _>>()?;
    let usage = response.get("usage").cloned();
    let model = response
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_else(|| strip_provider_prefix(&intent.model, "openrouter"))
        .to_owned();
    Ok(EmbeddingResult {
        embeddings,
        provider: ProviderKind::OpenRouter.label(),
        model,
        latency_ms: started.elapsed().as_millis(),
        prompt_tokens: usage_token(&usage, "prompt_tokens"),
        usage,
    })
}

async fn embed_ollama(
    intent: EmbeddingIntent,
    settings: ProviderSettings,
) -> Result<EmbeddingResult, ProviderTransportError> {
    let api_key = settings.ollama_key.filter(|key| !key.trim().is_empty());
    if is_ollama_cloud_base_url(&settings.ollama_base_url) && api_key.is_none() {
        return Err(ProviderTransportError::MissingCredential("Ollama"));
    }
    let body = json!({
        "model": strip_provider_prefix(&intent.model, "ollama"),
        "input": intent.input
    });
    let started = Instant::now();
    let response = post_json(&ollama_embed_url(&settings.ollama_base_url), api_key, body).await?;
    let embeddings = response
        .get("embeddings")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderTransportError::Malformed("missing Ollama embeddings".to_owned()))?
        .iter()
        .map(|item| decode_embedding(Some(item)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EmbeddingResult {
        embeddings,
        provider: ProviderKind::Ollama.label(),
        model: strip_provider_prefix(&intent.model, "ollama").to_owned(),
        latency_ms: started.elapsed().as_millis(),
        usage: None,
        prompt_tokens: response
            .get("prompt_eval_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

async fn post_json(
    url: &str,
    api_key: Option<String>,
    body: Value,
) -> Result<Value, ProviderTransportError> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| ProviderTransportError::Transport(e.to_string()))?;
    let mut request = client.post(url).json(&body);
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    if url.starts_with(OPENROUTER_BASE_URL) {
        request = request.header("X-Title", "Podcastr");
    }
    let response = request
        .send()
        .await
        .map_err(|e| ProviderTransportError::Transport(e.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| ProviderTransportError::Transport(e.to_string()))?;
    if !status.is_success() {
        return Err(ProviderTransportError::ProviderStatus(
            status.as_u16(),
            text,
        ));
    }
    serde_json::from_str(&text).map_err(|e| ProviderTransportError::Decode(e.to_string()))
}

fn decode_embedding(value: Option<&Value>) -> Result<Vec<f32>, ProviderTransportError> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderTransportError::Malformed("embedding is not an array".to_owned()))?
        .iter()
        .map(|item| {
            item.as_f64().map(|value| value as f32).ok_or_else(|| {
                ProviderTransportError::Malformed("embedding item is not numeric".to_owned())
            })
        })
        .collect()
}

fn usage_token(usage: &Option<Value>, key: &str) -> u64 {
    usage
        .as_ref()
        .and_then(|usage| usage.get(key))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_intent_decodes_json_format() {
        let intent: CompletionIntent = serde_json::from_value(json!({
            "provider": "openrouter",
            "model": "openai/gpt-4o-mini",
            "system": "sys",
            "user": "usr",
            "response_format": "json_object"
        }))
        .unwrap();
        assert_eq!(intent.provider, ProviderKind::OpenRouter);
        assert_eq!(intent.response_format, ResponseFormat::JsonObject);
    }
}
