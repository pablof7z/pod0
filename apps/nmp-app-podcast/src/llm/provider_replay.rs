//! Runtime cassette replay hooks for provider-backed validation.
//!
//! Replay mode is enabled by setting `POD0_PROVIDER_CASSETTE_DIR` to a
//! directory of verified provider cassettes. When enabled, a request miss is an
//! error instead of a live-provider fallback so validation stays hermetic.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde_json::Value;

use super::provider_cassette::{CassetteStore, ReplayResponse};

pub const CASSETTE_DIR_ENV: &str = "POD0_PROVIDER_CASSETTE_DIR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderReplayError {
    Load {
        dir: PathBuf,
        message: String,
    },
    Missing {
        provider: String,
        operation: String,
        method: String,
        url: String,
    },
    ProviderStatus {
        status: u16,
        body: String,
    },
    InvalidCassetteAudioSource(String),
}

impl std::fmt::Display for ProviderReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load { dir, message } => {
                write!(
                    f,
                    "provider cassette load failed at {}: {message}",
                    dir.display()
                )
            }
            Self::Missing {
                provider,
                operation,
                method,
                url,
            } => write!(
                f,
                "provider cassette replay miss for {provider}/{operation} {method} {url}"
            ),
            Self::ProviderStatus { status, body } => {
                write!(f, "provider cassette returned HTTP {status}: {body}")
            }
            Self::InvalidCassetteAudioSource(message) => {
                write!(f, "invalid cassette audio source: {message}")
            }
        }
    }
}

pub fn is_enabled() -> bool {
    cassette_dir().is_some()
}

pub fn lookup_json(
    provider: &str,
    operation: &str,
    method: &str,
    url: &str,
    body: &Value,
) -> Result<Option<ReplayResponse>, ProviderReplayError> {
    let Some(store) = replay_store()? else {
        return Ok(None);
    };
    Ok(Some(lookup_in_store(
        &store, provider, operation, method, url, body,
    )?))
}

pub fn lookup_json_in_dir(
    dir: impl AsRef<Path>,
    provider: &str,
    operation: &str,
    method: &str,
    url: &str,
    body: &Value,
) -> Result<ReplayResponse, ProviderReplayError> {
    let dir = dir.as_ref();
    let store = CassetteStore::load_dir(dir).map_err(|message| ProviderReplayError::Load {
        dir: dir.to_path_buf(),
        message,
    })?;
    lookup_in_store(&store, provider, operation, method, url, body)
}

fn lookup_in_store(
    store: &CassetteStore,
    provider: &str,
    operation: &str,
    method: &str,
    url: &str,
    body: &Value,
) -> Result<ReplayResponse, ProviderReplayError> {
    store
        .find(provider, operation, method, url, body)
        .ok_or_else(|| ProviderReplayError::Missing {
            provider: provider.to_owned(),
            operation: operation.to_owned(),
            method: method.to_owned(),
            url: url.to_owned(),
        })
}

fn replay_store() -> Result<Option<Arc<CassetteStore>>, ProviderReplayError> {
    static STORE: OnceLock<Result<Option<Arc<CassetteStore>>, ProviderReplayError>> =
        OnceLock::new();
    STORE
        .get_or_init(|| {
            let Some(dir) = cassette_dir() else {
                return Ok(None);
            };
            CassetteStore::load_dir(&dir)
                .map(Arc::new)
                .map(Some)
                .map_err(|message| ProviderReplayError::Load { dir, message })
        })
        .clone()
}

pub fn success_body(response: ReplayResponse) -> Result<(Value, u128), ProviderReplayError> {
    if !(200..300).contains(&response.status) {
        return Err(ProviderReplayError::ProviderStatus {
            status: response.status,
            body: response.body.to_string(),
        });
    }
    Ok((response.body, u128::from(response.replay_latency_ms)))
}

pub fn cassette_audio_sha256(source: &str) -> Option<String> {
    let url = url::Url::parse(source.trim()).ok()?;
    if url.scheme() != "cassette" {
        return None;
    }
    url.query_pairs()
        .find(|(key, _)| key == "sha256")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.trim().is_empty())
}

pub fn require_cassette_audio_sha256(source: &str) -> Result<String, ProviderReplayError> {
    cassette_audio_sha256(source).ok_or_else(|| {
        ProviderReplayError::InvalidCassetteAudioSource(
            "cassette:// audio URLs for multipart replay must include ?sha256=<hex>".to_owned(),
        )
    })
}

fn cassette_dir() -> Option<PathBuf> {
    env::var(CASSETTE_DIR_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_in_dir_returns_matching_cassette() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/provider_cassettes");
        let body = serde_json::json!({
            "model": "deepseek/deepseek-chat",
            "messages": [
                {"role": "system", "content": "You answer from the episode transcript only."},
                {"role": "user", "content": "What is the host's main takeaway?"}
            ],
            "stream": false
        });
        let response = lookup_json_in_dir(
            dir,
            "openrouter",
            "chat_completion",
            "POST",
            "https://openrouter.ai/api/v1/chat/completions",
            &body,
        )
        .unwrap();
        assert_eq!(response.cassette_id, "openrouter-agent-answer-success");
    }

    #[test]
    fn extracts_redacted_audio_hash_from_cassette_url() {
        assert_eq!(
            cassette_audio_sha256("cassette://audio/pod0.wav?sha256=abc123").as_deref(),
            Some("abc123")
        );
        assert!(cassette_audio_sha256("https://example.test/pod0.wav?sha256=abc123").is_none());
    }
}
