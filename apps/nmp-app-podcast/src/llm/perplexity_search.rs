//! Shared Perplexity/OpenRouter online-search transport.
//!
//! Platforms submit a search query. Rust owns provider selection, endpoints,
//! credentials, request bodies, HTTP status handling, and response parsing.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::provider_config::{ProviderConfigError, ProviderSettings, OPENROUTER_BASE_URL};
use super::provider_replay::{self, ProviderReplayError};
use crate::store::PodcastStore;

const PERPLEXITY_SONAR_URL: &str = "https://api.perplexity.ai/v1/sonar";
const DEFAULT_PERPLEXITY_MODEL: &str = "sonar";
const OPENROUTER_PERPLEXITY_MODEL: &str = "perplexity/sonar";
const SEARCH_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Deserialize)]
pub struct PerplexitySearchIntent {
    pub query: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PerplexitySearchResult {
    pub answer: String,
    pub sources: Vec<PerplexitySource>,
    pub provider: &'static str,
    pub model: String,
    pub latency_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PerplexitySource {
    pub title: String,
    pub url: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PerplexitySearchError {
    InvalidQuery,
    MissingCredential,
    Timeout,
    Transport(String),
    ProviderStatus(u16, String),
    Decode(String),
    Malformed(String),
    StoreUnavailable,
}

impl PerplexitySearchError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidQuery => "invalid_query",
            Self::MissingCredential => "missing_api_key",
            Self::Timeout => "timed_out",
            Self::Transport(_) => "network_error",
            Self::ProviderStatus(401 | 403, _) => "invalid_key",
            Self::ProviderStatus(429, _) => "rate_limited",
            Self::ProviderStatus(_, _) => "server_error",
            Self::Decode(_) => "decoding_error",
            Self::Malformed(_) => "malformed_response",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::ProviderStatus(status, _) => Some(*status),
            _ => None,
        }
    }
}

impl std::fmt::Display for PerplexitySearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidQuery => write!(f, "search query is empty"),
            Self::MissingCredential => {
                write!(f, "No Perplexity or OpenRouter API key is configured")
            }
            Self::Timeout => write!(f, "online search timed out"),
            Self::Transport(message) => write!(f, "online search failed: {message}"),
            Self::ProviderStatus(status, body) => write!(
                f,
                "online search returned HTTP {status}: {}",
                body.chars().take(300).collect::<String>()
            ),
            Self::Decode(message) => write!(f, "online search decode failed: {message}"),
            Self::Malformed(message) => write!(f, "online search response malformed: {message}"),
            Self::StoreUnavailable => write!(f, "settings store unavailable"),
        }
    }
}

impl From<ProviderConfigError> for PerplexitySearchError {
    fn from(error: ProviderConfigError) -> Self {
        match error {
            ProviderConfigError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

pub async fn search_perplexity(
    store: Arc<Mutex<PodcastStore>>,
    intent: PerplexitySearchIntent,
) -> Result<PerplexitySearchResult, PerplexitySearchError> {
    let query = intent.query.trim();
    if query.is_empty() {
        return Err(PerplexitySearchError::InvalidQuery);
    }
    let settings = ProviderSettings::from_store(&store)?;
    if provider_replay::is_enabled() {
        let body = direct_perplexity_body(query);
        if let Some(response) = provider_replay::lookup_json(
            "perplexity",
            "web_search",
            "POST",
            PERPLEXITY_SONAR_URL,
            &body,
        )
        .map_err(replay_search_error)?
        {
            let (response, latency_ms) =
                provider_replay::success_body(response).map_err(replay_search_error)?;
            return decode_search_response(
                response,
                "perplexity",
                DEFAULT_PERPLEXITY_MODEL,
                latency_ms,
            );
        }
    }
    if let Some(key) = settings.perplexity_key.filter(|key| !key.trim().is_empty()) {
        return search_direct_perplexity(query, key).await;
    }
    if let Some(key) = settings.openrouter_key.filter(|key| !key.trim().is_empty()) {
        return search_openrouter_perplexity(query, key).await;
    }
    Err(PerplexitySearchError::MissingCredential)
}

async fn search_direct_perplexity(
    query: &str,
    api_key: String,
) -> Result<PerplexitySearchResult, PerplexitySearchError> {
    let body = direct_perplexity_body(query);
    let started = Instant::now();
    let response = post_json(PERPLEXITY_SONAR_URL, api_key, body).await?;
    decode_search_response(
        response,
        "perplexity",
        DEFAULT_PERPLEXITY_MODEL,
        started.elapsed().as_millis(),
    )
}

fn direct_perplexity_body(query: &str) -> Value {
    json!({
        "model": DEFAULT_PERPLEXITY_MODEL,
        "messages": [{"role": "user", "content": query}],
    })
}

async fn search_openrouter_perplexity(
    query: &str,
    api_key: String,
) -> Result<PerplexitySearchResult, PerplexitySearchError> {
    let body = json!({
        "model": OPENROUTER_PERPLEXITY_MODEL,
        "messages": [{"role": "user", "content": query}],
        "return_citations": true,
    });
    let started = Instant::now();
    let response = post_json(
        &format!("{OPENROUTER_BASE_URL}/chat/completions"),
        api_key,
        body,
    )
    .await?;
    decode_search_response(
        response,
        "openrouter",
        OPENROUTER_PERPLEXITY_MODEL,
        started.elapsed().as_millis(),
    )
}

async fn post_json(
    url: &str,
    api_key: String,
    body: Value,
) -> Result<Value, PerplexitySearchError> {
    let client = reqwest::Client::builder()
        .timeout(SEARCH_TIMEOUT)
        .build()
        .map_err(|e| PerplexitySearchError::Transport(e.to_string()))?;
    let mut request = client
        .post(url)
        .bearer_auth(api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body);
    if url.starts_with(OPENROUTER_BASE_URL) {
        request = request.header("X-Title", "Podcastr");
    }
    let response = request.send().await.map_err(map_reqwest_error)?;
    let status = response.status();
    let text = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(PerplexitySearchError::ProviderStatus(status.as_u16(), text));
    }
    serde_json::from_str(&text).map_err(|e| PerplexitySearchError::Decode(e.to_string()))
}

fn map_reqwest_error(error: reqwest::Error) -> PerplexitySearchError {
    if error.is_timeout() {
        PerplexitySearchError::Timeout
    } else {
        PerplexitySearchError::Transport(error.to_string())
    }
}

fn decode_search_response(
    value: Value,
    provider: &'static str,
    fallback_model: &str,
    latency_ms: u128,
) -> Result<PerplexitySearchResult, PerplexitySearchError> {
    let answer = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| PerplexitySearchError::Malformed("missing assistant content".to_owned()))?
        .to_owned();
    let sources = decode_sources(&value);
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(fallback_model)
        .to_owned();
    Ok(PerplexitySearchResult {
        answer,
        sources,
        provider,
        model,
        latency_ms,
        usage: value.get("usage").cloned(),
    })
}

fn decode_sources(value: &Value) -> Vec<PerplexitySource> {
    if let Some(results) = value.get("search_results").and_then(Value::as_array) {
        let sources: Vec<PerplexitySource> = results
            .iter()
            .filter_map(|result| {
                let url = result.get("url").and_then(Value::as_str)?;
                let title = result.get("title").and_then(Value::as_str).unwrap_or(url);
                Some(PerplexitySource {
                    title: title.to_owned(),
                    url: url.to_owned(),
                })
            })
            .collect();
        if !sources.is_empty() {
            return sources;
        }
    }
    value
        .get("citations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|url| PerplexitySource {
            title: url.to_owned(),
            url: url.to_owned(),
        })
        .collect()
}

fn replay_search_error(error: ProviderReplayError) -> PerplexitySearchError {
    match error {
        ProviderReplayError::ProviderStatus { status, body } => {
            PerplexitySearchError::ProviderStatus(status, body)
        }
        ProviderReplayError::InvalidCassetteAudioSource(message) => {
            PerplexitySearchError::Malformed(message)
        }
        error => PerplexitySearchError::Transport(error.to_string()),
    }
}

#[cfg(test)]
#[path = "perplexity_search_tests.rs"]
mod tests;
