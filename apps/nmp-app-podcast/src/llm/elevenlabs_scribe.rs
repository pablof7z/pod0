//! Shared ElevenLabs Scribe transcription transport.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};

use super::provider_config::{
    strip_provider_prefix, ProviderConfigError, ProviderSettings, ELEVENLABS_BASE_URL,
};
use crate::store::PodcastStore;

const TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(600);
const DEFAULT_SCRIBE_MODEL: &str = "scribe_v1";

#[derive(Debug, Deserialize)]
pub struct ElevenLabsScribeIntent {
    pub audio_url: String,
    #[serde(default)]
    pub language_hint: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ElevenLabsScribeResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub words: Vec<ElevenLabsScribeWord>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    pub latency_ms: u128,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ElevenLabsScribeWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_id: Option<String>,
}

#[derive(Debug)]
pub enum ElevenLabsScribeError {
    MissingCredential,
    InvalidAudioSource(String),
    Timeout,
    Transport(String),
    ProviderStatus(u16, String),
    Decode(String),
    StoreUnavailable,
}

impl ElevenLabsScribeError {
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

impl std::fmt::Display for ElevenLabsScribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredential => write!(f, "ElevenLabs API key is not configured"),
            Self::InvalidAudioSource(message) => write!(f, "invalid audio source: {message}"),
            Self::Timeout => write!(f, "ElevenLabs Scribe transcription timed out"),
            Self::Transport(message) => write!(f, "ElevenLabs Scribe failed: {message}"),
            Self::ProviderStatus(status, body) => write!(
                f,
                "ElevenLabs Scribe returned HTTP {status}: {}",
                body.chars().take(300).collect::<String>()
            ),
            Self::Decode(message) => write!(f, "ElevenLabs Scribe decode failed: {message}"),
            Self::StoreUnavailable => write!(f, "settings store unavailable"),
        }
    }
}

impl From<ProviderConfigError> for ElevenLabsScribeError {
    fn from(error: ProviderConfigError) -> Self {
        match error {
            ProviderConfigError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

pub async fn transcribe_elevenlabs_scribe(
    store: Arc<Mutex<PodcastStore>>,
    intent: ElevenLabsScribeIntent,
) -> Result<ElevenLabsScribeResult, ElevenLabsScribeError> {
    let settings = ProviderSettings::from_store(&store)?;
    let api_key = settings
        .eleven_labs_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(ElevenLabsScribeError::MissingCredential)?;
    let model = normalize_model(&settings.eleven_labs_stt_model);
    let client = reqwest::Client::builder()
        .timeout(TRANSCRIPTION_TIMEOUT)
        .build()
        .map_err(map_reqwest_error)?;
    let audio = resolve_scribe_audio_source(&intent.audio_url).await?;
    let form = build_scribe_form(model.clone(), intent.language_hint, audio)?;
    let started = Instant::now();
    let response = client
        .post(format!("{ELEVENLABS_BASE_URL}/v1/speech-to-text"))
        .header("xi-api-key", api_key)
        .header("Accept", "application/json")
        .multipart(form)
        .timeout(TRANSCRIPTION_TIMEOUT)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    let status = response.status();
    let text = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(ElevenLabsScribeError::ProviderStatus(status.as_u16(), text));
    }
    decode_scribe_response(&text, model, started.elapsed().as_millis())
}

fn normalize_model(raw: &str) -> String {
    let trimmed = strip_provider_prefix(raw.trim(), "elevenlabs").trim();
    if trimmed.is_empty() {
        DEFAULT_SCRIBE_MODEL.to_owned()
    } else {
        trimmed.to_owned()
    }
}

enum ScribeAudioSource {
    File {
        bytes: Vec<u8>,
        filename: String,
        content_type: String,
    },
    SourceUrl(String),
}

async fn resolve_scribe_audio_source(
    source: &str,
) -> Result<ScribeAudioSource, ElevenLabsScribeError> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(ElevenLabsScribeError::InvalidAudioSource(
            "empty audio source".to_owned(),
        ));
    }
    if let Ok(url) = url::Url::parse(trimmed) {
        return match url.scheme() {
            "file" => {
                let path = url.to_file_path().map_err(|_| {
                    ElevenLabsScribeError::InvalidAudioSource("invalid file URL".to_owned())
                })?;
                read_local_audio(path).await
            }
            "http" | "https" => Ok(ScribeAudioSource::SourceUrl(trimmed.to_owned())),
            scheme => Err(ElevenLabsScribeError::InvalidAudioSource(format!(
                "unsupported URL scheme {scheme}"
            ))),
        };
    }
    read_local_audio(PathBuf::from(trimmed)).await
}

async fn read_local_audio(path: PathBuf) -> Result<ScribeAudioSource, ElevenLabsScribeError> {
    if !path.exists() {
        return Err(ElevenLabsScribeError::InvalidAudioSource(format!(
            "{} does not exist",
            path.display()
        )));
    }
    let filename = filename_from_path(&path);
    let content_type = content_type_for_extension(path.extension().and_then(|ext| ext.to_str()));
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .map_err(|e| ElevenLabsScribeError::Transport(e.to_string()))?
        .map_err(|e| ElevenLabsScribeError::InvalidAudioSource(e.to_string()))?;
    Ok(ScribeAudioSource::File {
        bytes,
        filename,
        content_type,
    })
}

fn build_scribe_form(
    model: String,
    language_hint: Option<String>,
    audio: ScribeAudioSource,
) -> Result<Form, ElevenLabsScribeError> {
    let mut form = Form::new()
        .text("model_id", model)
        .text("diarize", "true")
        .text("timestamps_granularity", "word")
        .text("tag_audio_events", "true");
    if let Some(language) = language_hint.filter(|hint| !hint.trim().is_empty()) {
        form = form.text("language_code", language);
    }
    match audio {
        ScribeAudioSource::SourceUrl(url) => Ok(form.text("source_url", url)),
        ScribeAudioSource::File {
            bytes,
            filename,
            content_type,
        } => {
            let part = Part::bytes(bytes)
                .file_name(filename)
                .mime_str(&content_type)
                .map_err(|e| ElevenLabsScribeError::InvalidAudioSource(e.to_string()))?;
            Ok(form.part("file", part))
        }
    }
}

#[derive(Debug, Deserialize)]
struct ScribeResponse {
    language_code: Option<String>,
    text: Option<String>,
    words: Option<Vec<ElevenLabsScribeWord>>,
}

fn decode_scribe_response(
    text: &str,
    model: String,
    latency_ms: u128,
) -> Result<ElevenLabsScribeResult, ElevenLabsScribeError> {
    let raw: ScribeResponse =
        serde_json::from_str(text).map_err(|e| ElevenLabsScribeError::Decode(e.to_string()))?;
    let words = raw.words.unwrap_or_default();
    let duration = words.iter().map(|word| word.end).reduce(f64::max);
    Ok(ElevenLabsScribeResult {
        language_code: raw.language_code,
        text: raw.text,
        words,
        model,
        duration,
        latency_ms,
    })
}

fn filename_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("audio.mp3")
        .to_owned()
}

fn content_type_for_extension(extension: Option<&str>) -> String {
    match extension.unwrap_or_default().to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "m4a" | "m4b" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "opus" => "audio/opus",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    }
    .to_owned()
}

fn map_reqwest_error(error: reqwest::Error) -> ElevenLabsScribeError {
    if error.is_timeout() {
        ElevenLabsScribeError::Timeout
    } else {
        ElevenLabsScribeError::Transport(error.to_string())
    }
}

#[cfg(test)]
mod tests;
