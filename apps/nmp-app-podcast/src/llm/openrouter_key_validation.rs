//! Shared OpenRouter credential validation transport.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::provider_config::{ProviderConfigError, ProviderSettings, OPENROUTER_BASE_URL};
use crate::store::PodcastStore;

const AUTH_KEY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OpenRouterKeyInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_dollars: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_dollars: Option<f64>,
    pub is_free_tier: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_interval: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_interval: Option<String>,
}

#[derive(Debug)]
pub enum OpenRouterKeyValidationError {
    MissingCredential,
    InvalidKey,
    Transport(String),
    ProviderStatus(u16, String),
    Decode(String),
    StoreUnavailable,
}

impl OpenRouterKeyValidationError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::MissingCredential => "missing_api_key",
            Self::InvalidKey => "invalid_key",
            Self::Transport(_) => "network_error",
            Self::ProviderStatus(_, _) => "server_error",
            Self::Decode(_) => "decoding_error",
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

impl std::fmt::Display for OpenRouterKeyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredential => write!(f, "OpenRouter API key is not configured"),
            Self::InvalidKey => write!(f, "OpenRouter rejected the API key"),
            Self::Transport(message) => write!(f, "OpenRouter validation failed: {message}"),
            Self::ProviderStatus(status, body) => {
                write!(
                    f,
                    "OpenRouter validation returned HTTP {status}: {}",
                    body.chars().take(300).collect::<String>()
                )
            }
            Self::Decode(message) => {
                write!(f, "OpenRouter validation response decode failed: {message}")
            }
            Self::StoreUnavailable => write!(f, "settings store unavailable"),
        }
    }
}

impl From<ProviderConfigError> for OpenRouterKeyValidationError {
    fn from(error: ProviderConfigError) -> Self {
        match error {
            ProviderConfigError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

pub async fn validate_openrouter_key(
    store: Arc<Mutex<PodcastStore>>,
) -> Result<OpenRouterKeyInfo, OpenRouterKeyValidationError> {
    let settings = ProviderSettings::from_store(&store)?;
    let api_key = settings
        .openrouter_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(OpenRouterKeyValidationError::MissingCredential)?;
    let client = reqwest::Client::builder()
        .timeout(AUTH_KEY_TIMEOUT)
        .build()
        .map_err(|e| OpenRouterKeyValidationError::Transport(e.to_string()))?;
    validate_openrouter_key_with_client(
        &client,
        &format!("{OPENROUTER_BASE_URL}/auth/key"),
        &api_key,
    )
    .await
}

async fn validate_openrouter_key_with_client(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> Result<OpenRouterKeyInfo, OpenRouterKeyValidationError> {
    let response = client
        .get(url)
        .bearer_auth(api_key)
        .header("X-Title", "Podcastr")
        .timeout(AUTH_KEY_TIMEOUT)
        .send()
        .await
        .map_err(|e| OpenRouterKeyValidationError::Transport(e.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| OpenRouterKeyValidationError::Transport(e.to_string()))?;
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(OpenRouterKeyValidationError::InvalidKey);
    }
    if !status.is_success() {
        return Err(OpenRouterKeyValidationError::ProviderStatus(
            status.as_u16(),
            text,
        ));
    }
    decode_auth_key_response(&text)
}

fn decode_auth_key_response(text: &str) -> Result<OpenRouterKeyInfo, OpenRouterKeyValidationError> {
    let dto: AuthKeyResponse = serde_json::from_str(text)
        .map_err(|e| OpenRouterKeyValidationError::Decode(e.to_string()))?;
    Ok(OpenRouterKeyInfo {
        label: dto.data.label,
        usage_dollars: dto.data.usage,
        limit_dollars: dto.data.limit,
        is_free_tier: dto.data.is_free_tier,
        requests_per_interval: dto.data.rate_limit.as_ref().and_then(|rate| rate.requests),
        rate_interval: dto.data.rate_limit.and_then(|rate| rate.interval),
    })
}

#[derive(Debug, Deserialize)]
struct AuthKeyResponse {
    data: AuthKeyData,
}

#[derive(Debug, Deserialize)]
struct AuthKeyData {
    label: Option<String>,
    usage: Option<f64>,
    limit: Option<f64>,
    #[serde(default)]
    is_free_tier: bool,
    rate_limit: Option<AuthKeyRateLimit>,
}

#[derive(Debug, Deserialize)]
struct AuthKeyRateLimit {
    requests: Option<i64>,
    interval: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_openrouter_auth_key_response() {
        let info = decode_auth_key_response(
            r#"{
                "data": {
                    "label": "podcast",
                    "usage": 1.25,
                    "limit": 10.0,
                    "is_free_tier": false,
                    "rate_limit": {"requests": 20, "interval": "10s"}
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            info,
            OpenRouterKeyInfo {
                label: Some("podcast".to_owned()),
                usage_dollars: Some(1.25),
                limit_dollars: Some(10.0),
                is_free_tier: false,
                requests_per_interval: Some(20),
                rate_interval: Some("10s".to_owned()),
            }
        );
    }

    #[test]
    fn invalid_key_error_has_stable_ffi_kind() {
        assert_eq!(
            OpenRouterKeyValidationError::InvalidKey.kind(),
            "invalid_key"
        );
    }
}
