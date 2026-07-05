//! Shared AssemblyAI pre-recorded transcription transport.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::provider_config::{
    strip_provider_prefix, ProviderConfigError, ProviderSettings, ASSEMBLYAI_BASE_URL,
};
use super::provider_replay::{self, ProviderReplayError};
use crate::store::PodcastStore;

mod types;
use types::{AssemblyAIResponse, SubmitRequest};
pub use types::{
    AssemblyAITranscriptIntent, AssemblyAITranscriptResult, AssemblyAIUsage, AssemblyAIUtterance,
    AssemblyAIWord,
};

const SUBMIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const POLL_TIMEOUT: Duration = Duration::from_secs(1_800);
const DEFAULT_MODELS: &str = "universal-3-pro,universal-2";

#[derive(Debug)]
pub enum AssemblyAITranscriptError {
    MissingCredential,
    InvalidAudioSource(String),
    Timeout,
    Transport(String),
    ProviderStatus(u16, String),
    Decode(String),
    Remote(String),
    StoreUnavailable,
}

impl AssemblyAITranscriptError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::MissingCredential => "missing_api_key",
            Self::InvalidAudioSource(_) => "invalid_audio_url",
            Self::Timeout => "timed_out",
            Self::Transport(_) => "network_error",
            Self::ProviderStatus(401 | 403, _) => "invalid_key",
            Self::ProviderStatus(429, _) => "rate_limited",
            Self::ProviderStatus(_, _) => "server_error",
            Self::Decode(_) => "decoding_error",
            Self::Remote(_) => "remote_error",
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

impl std::fmt::Display for AssemblyAITranscriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredential => write!(f, "AssemblyAI API key is not configured"),
            Self::InvalidAudioSource(message) => write!(f, "invalid audio source: {message}"),
            Self::Timeout => write!(f, "AssemblyAI transcription timed out"),
            Self::Transport(message) => write!(f, "AssemblyAI transcription failed: {message}"),
            Self::ProviderStatus(status, body) => write!(
                f,
                "AssemblyAI transcription returned HTTP {status}: {}",
                body.chars().take(300).collect::<String>()
            ),
            Self::Decode(message) => write!(f, "AssemblyAI transcript decode failed: {message}"),
            Self::Remote(message) => write!(f, "AssemblyAI transcript failed: {message}"),
            Self::StoreUnavailable => write!(f, "settings store unavailable"),
        }
    }
}

impl From<ProviderConfigError> for AssemblyAITranscriptError {
    fn from(error: ProviderConfigError) -> Self {
        match error {
            ProviderConfigError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

pub async fn transcribe_assemblyai(
    store: Arc<Mutex<PodcastStore>>,
    intent: AssemblyAITranscriptIntent,
) -> Result<AssemblyAITranscriptResult, AssemblyAITranscriptError> {
    let settings = ProviderSettings::from_store(&store)?;
    let models = speech_models(&settings.assembly_ai_stt_model);
    let submit_url = format!("{ASSEMBLYAI_BASE_URL}/v2/transcript");
    if provider_replay::is_enabled() {
        let body = replay_submit_body(&intent.audio_url, &models, intent.language_hint.as_deref())?;
        if let Some(response) = provider_replay::lookup_json(
            "assemblyai",
            "stt_transcription",
            "POST",
            &submit_url,
            &body,
        )
        .map_err(replay_assemblyai_error)?
        {
            let (body, latency_ms) =
                provider_replay::success_body(response).map_err(replay_assemblyai_error)?;
            let raw: AssemblyAIResponse = serde_json::from_value(body)
                .map_err(|e| AssemblyAITranscriptError::Decode(e.to_string()))?;
            return Ok(raw.into_result(models.join(","), latency_ms));
        }
    }
    let api_key = settings
        .assembly_ai_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(AssemblyAITranscriptError::MissingCredential)?;
    let audio_url = remote_audio_url(&intent.audio_url)?;
    let client = reqwest::Client::builder()
        .timeout(SUBMIT_TIMEOUT)
        .build()
        .map_err(map_reqwest_error)?;
    let started = Instant::now();
    let transcript_id = submit_transcript(&client, &api_key, audio_url, &models, intent).await?;
    poll_transcript(&client, &api_key, &transcript_id, &models, started).await
}

fn replay_submit_body(
    audio_url: &str,
    models: &[String],
    language_hint: Option<&str>,
) -> Result<serde_json::Value, AssemblyAITranscriptError> {
    let language_code = language_hint
        .filter(|hint| !hint.trim().is_empty())
        .map(str::to_owned);
    serde_json::to_value(SubmitRequest {
        audio_url: audio_url.to_owned(),
        speech_models: models.to_vec(),
        speaker_labels: true,
        language_detection: language_code.is_none().then_some(true),
        language_code,
    })
    .map_err(|e| AssemblyAITranscriptError::Decode(e.to_string()))
}

fn speech_models(raw: &str) -> Vec<String> {
    let trimmed = strip_provider_prefix(raw.trim(), "assemblyai").trim();
    let source = if trimmed.is_empty() {
        DEFAULT_MODELS
    } else {
        trimmed
    };
    let models: Vec<String> = source
        .split(',')
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .collect();
    if models.is_empty() {
        DEFAULT_MODELS.split(',').map(str::to_owned).collect()
    } else {
        models
    }
}

fn remote_audio_url(source: &str) -> Result<String, AssemblyAITranscriptError> {
    let trimmed = source.trim();
    let url = url::Url::parse(trimmed)
        .map_err(|_| AssemblyAITranscriptError::InvalidAudioSource("invalid URL".to_owned()))?;
    match url.scheme() {
        "http" | "https" => Ok(trimmed.to_owned()),
        scheme => Err(AssemblyAITranscriptError::InvalidAudioSource(format!(
            "unsupported URL scheme {scheme}"
        ))),
    }
}

async fn submit_transcript(
    client: &reqwest::Client,
    api_key: &str,
    audio_url: String,
    models: &[String],
    intent: AssemblyAITranscriptIntent,
) -> Result<String, AssemblyAITranscriptError> {
    let language_code = intent.language_hint.filter(|hint| !hint.trim().is_empty());
    let body = SubmitRequest {
        audio_url,
        speech_models: models.to_vec(),
        speaker_labels: true,
        language_detection: language_code.is_none().then_some(true),
        language_code,
    };
    let response = client
        .post(format!("{ASSEMBLYAI_BASE_URL}/v2/transcript"))
        .header("Authorization", api_key)
        .header("Accept", "application/json")
        .json(&body)
        .timeout(SUBMIT_TIMEOUT)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    let payload = response_text(response).await?;
    let raw: AssemblyAIResponse = decode_response(&payload)?;
    raw.id.ok_or(AssemblyAITranscriptError::Decode(
        "submit response missing id".to_owned(),
    ))
}

async fn poll_transcript(
    client: &reqwest::Client,
    api_key: &str,
    transcript_id: &str,
    models: &[String],
    started: Instant,
) -> Result<AssemblyAITranscriptResult, AssemblyAITranscriptError> {
    let deadline = Instant::now() + POLL_TIMEOUT;
    while Instant::now() < deadline {
        let response = client
            .get(format!(
                "{ASSEMBLYAI_BASE_URL}/v2/transcript/{transcript_id}"
            ))
            .header("Authorization", api_key)
            .header("Accept", "application/json")
            .timeout(SUBMIT_TIMEOUT)
            .send()
            .await;
        let text = match response {
            Ok(response) => response_text(response).await?,
            Err(error) if error.is_timeout() => {
                tokio::time::sleep(POLL_INTERVAL).await;
                continue;
            }
            Err(error) => return Err(map_reqwest_error(error)),
        };
        let raw: AssemblyAIResponse = decode_response(&text)?;
        match raw.status.as_deref() {
            Some("completed") => {
                return Ok(raw.into_result(models.join(","), started.elapsed().as_millis()));
            }
            Some("error") => {
                return Err(AssemblyAITranscriptError::Remote(raw.error.unwrap_or_else(
                    || "AssemblyAI returned status=error without a message".to_owned(),
                )));
            }
            _ => tokio::time::sleep(POLL_INTERVAL).await,
        }
    }
    Err(AssemblyAITranscriptError::Timeout)
}

async fn response_text(response: reqwest::Response) -> Result<String, AssemblyAITranscriptError> {
    let status = response.status();
    let text = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(AssemblyAITranscriptError::ProviderStatus(
            status.as_u16(),
            text,
        ));
    }
    Ok(text)
}

fn decode_response<T: serde::de::DeserializeOwned>(
    text: &str,
) -> Result<T, AssemblyAITranscriptError> {
    serde_json::from_str(text).map_err(|e| AssemblyAITranscriptError::Decode(e.to_string()))
}

fn map_reqwest_error(error: reqwest::Error) -> AssemblyAITranscriptError {
    if error.is_timeout() {
        AssemblyAITranscriptError::Timeout
    } else {
        AssemblyAITranscriptError::Transport(error.to_string())
    }
}

fn replay_assemblyai_error(error: ProviderReplayError) -> AssemblyAITranscriptError {
    match error {
        ProviderReplayError::ProviderStatus { status, body } => {
            AssemblyAITranscriptError::ProviderStatus(status, body)
        }
        ProviderReplayError::InvalidCassetteAudioSource(message) => {
            AssemblyAITranscriptError::InvalidAudioSource(message)
        }
        error => AssemblyAITranscriptError::Transport(error.to_string()),
    }
}

#[cfg(test)]
mod tests;
