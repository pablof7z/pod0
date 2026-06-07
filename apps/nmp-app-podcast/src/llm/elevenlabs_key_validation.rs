//! Shared ElevenLabs credential validation transport.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::provider_config::{ProviderConfigError, ProviderSettings, ELEVENLABS_BASE_URL};
use crate::store::PodcastStore;

const USER_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ElevenLabsKeyInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_limit: Option<i64>,
}

#[derive(Debug)]
pub enum ElevenLabsKeyValidationError {
    MissingCredential,
    InvalidKey,
    Transport(String),
    ProviderStatus(u16, String),
    Decode(String),
    StoreUnavailable,
}

impl ElevenLabsKeyValidationError {
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

impl std::fmt::Display for ElevenLabsKeyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredential => write!(f, "ElevenLabs API key is not configured"),
            Self::InvalidKey => write!(f, "ElevenLabs rejected the API key"),
            Self::Transport(message) => write!(f, "ElevenLabs validation failed: {message}"),
            Self::ProviderStatus(status, body) => {
                write!(
                    f,
                    "ElevenLabs validation returned HTTP {status}: {}",
                    body.chars().take(300).collect::<String>()
                )
            }
            Self::Decode(message) => {
                write!(f, "ElevenLabs validation response decode failed: {message}")
            }
            Self::StoreUnavailable => write!(f, "settings store unavailable"),
        }
    }
}

impl From<ProviderConfigError> for ElevenLabsKeyValidationError {
    fn from(error: ProviderConfigError) -> Self {
        match error {
            ProviderConfigError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

pub async fn validate_elevenlabs_key(
    store: Arc<Mutex<PodcastStore>>,
) -> Result<ElevenLabsKeyInfo, ElevenLabsKeyValidationError> {
    let settings = ProviderSettings::from_store(&store)?;
    let api_key = settings
        .eleven_labs_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(ElevenLabsKeyValidationError::MissingCredential)?;
    let client = reqwest::Client::builder()
        .timeout(USER_TIMEOUT)
        .build()
        .map_err(|e| ElevenLabsKeyValidationError::Transport(e.to_string()))?;
    validate_elevenlabs_key_with_client(
        &client,
        &format!("{ELEVENLABS_BASE_URL}/v1/user"),
        &api_key,
    )
    .await
}

async fn validate_elevenlabs_key_with_client(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> Result<ElevenLabsKeyInfo, ElevenLabsKeyValidationError> {
    let response = client
        .get(url)
        .header("xi-api-key", api_key)
        .timeout(USER_TIMEOUT)
        .send()
        .await
        .map_err(|e| ElevenLabsKeyValidationError::Transport(e.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| ElevenLabsKeyValidationError::Transport(e.to_string()))?;
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(ElevenLabsKeyValidationError::InvalidKey);
    }
    if !status.is_success() {
        return Err(ElevenLabsKeyValidationError::ProviderStatus(
            status.as_u16(),
            text,
        ));
    }
    decode_user_response(&text)
}

fn decode_user_response(text: &str) -> Result<ElevenLabsKeyInfo, ElevenLabsKeyValidationError> {
    let dto: UserResponse = serde_json::from_str(text)
        .map_err(|e| ElevenLabsKeyValidationError::Decode(e.to_string()))?;
    Ok(ElevenLabsKeyInfo {
        tier: dto
            .subscription
            .as_ref()
            .and_then(|subscription| subscription.tier.clone()),
        character_count: dto
            .subscription
            .as_ref()
            .and_then(|subscription| subscription.character_count),
        character_limit: dto
            .subscription
            .and_then(|subscription| subscription.character_limit),
    })
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    subscription: Option<UserSubscription>,
}

#[derive(Debug, Deserialize)]
struct UserSubscription {
    tier: Option<String>,
    character_count: Option<i64>,
    character_limit: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_elevenlabs_user_response() {
        let info = decode_user_response(
            r#"{
                "subscription": {
                    "tier": "creator",
                    "character_count": 1234,
                    "character_limit": 100000
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            info,
            ElevenLabsKeyInfo {
                tier: Some("creator".to_owned()),
                character_count: Some(1234),
                character_limit: Some(100000),
            }
        );
    }

    #[test]
    fn invalid_key_error_has_stable_ffi_kind() {
        assert_eq!(
            ElevenLabsKeyValidationError::InvalidKey.kind(),
            "invalid_key"
        );
    }
}
