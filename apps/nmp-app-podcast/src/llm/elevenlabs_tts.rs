//! Shared ElevenLabs one-shot text-to-speech transport.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use url::Url;

use super::provider_config::{ProviderConfigError, ProviderSettings, ELEVENLABS_BASE_URL};
use crate::store::PodcastStore;

const TTS_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_TTS_MODEL: &str = "eleven_turbo_v2_5";

#[derive(Debug, Deserialize)]
pub struct ElevenLabsTtsIntent {
    pub text: String,
    pub voice_id: String,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ElevenLabsTtsResult {
    pub audio_base64: String,
    pub content_type: String,
    pub model: String,
    pub voice_id: String,
    pub latency_ms: u128,
}

#[derive(Debug)]
pub enum ElevenLabsTtsError {
    MissingCredential,
    InvalidRequest(String),
    EmptyAudio,
    Transport(String),
    ProviderStatus(u16, String),
    StoreUnavailable,
}

impl ElevenLabsTtsError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::MissingCredential => "missing_api_key",
            Self::InvalidRequest(_) => "invalid_request",
            Self::EmptyAudio => "empty_audio",
            Self::Transport(_) => "network_error",
            Self::ProviderStatus(401 | 403, _) => "invalid_key",
            Self::ProviderStatus(429, _) => "rate_limited",
            Self::ProviderStatus(_, _) => "server_error",
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

impl std::fmt::Display for ElevenLabsTtsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredential => write!(f, "ElevenLabs API key is not configured"),
            Self::InvalidRequest(message) => write!(f, "invalid ElevenLabs TTS request: {message}"),
            Self::EmptyAudio => write!(f, "ElevenLabs TTS returned no audio"),
            Self::Transport(message) => write!(f, "ElevenLabs TTS failed: {message}"),
            Self::ProviderStatus(status, body) => write!(
                f,
                "ElevenLabs TTS returned HTTP {status}: {}",
                body.chars().take(300).collect::<String>()
            ),
            Self::StoreUnavailable => write!(f, "settings store unavailable"),
        }
    }
}

impl From<ProviderConfigError> for ElevenLabsTtsError {
    fn from(error: ProviderConfigError) -> Self {
        match error {
            ProviderConfigError::StoreUnavailable => Self::StoreUnavailable,
        }
    }
}

pub async fn synthesize_elevenlabs_tts(
    store: Arc<Mutex<PodcastStore>>,
    intent: ElevenLabsTtsIntent,
) -> Result<ElevenLabsTtsResult, ElevenLabsTtsError> {
    let settings = ProviderSettings::from_store(&store)?;
    let api_key = settings
        .eleven_labs_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(ElevenLabsTtsError::MissingCredential)?;
    let normalized = NormalizedTtsIntent::from_intent(intent, &settings.eleven_labs_tts_model)?;
    let client = reqwest::Client::builder()
        .timeout(TTS_TIMEOUT)
        .build()
        .map_err(map_reqwest_error)?;
    synthesize_elevenlabs_tts_with_client(&client, &api_key, normalized).await
}

async fn synthesize_elevenlabs_tts_with_client(
    client: &reqwest::Client,
    api_key: &str,
    intent: NormalizedTtsIntent,
) -> Result<ElevenLabsTtsResult, ElevenLabsTtsError> {
    let url = tts_url(&intent.voice_id)?;
    let started = Instant::now();
    let response = client
        .post(url)
        .header("xi-api-key", api_key)
        .header("Accept", "audio/mpeg")
        .json(&TtsRequestBody {
            text: &intent.text,
            model_id: &intent.model,
            voice_settings: TtsVoiceSettings {
                stability: 0.5,
                similarity_boost: 0.75,
            },
        })
        .timeout(TTS_TIMEOUT)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("audio/mpeg")
        .to_owned();
    let bytes = response.bytes().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(ElevenLabsTtsError::ProviderStatus(
            status.as_u16(),
            String::from_utf8_lossy(&bytes).to_string(),
        ));
    }
    if bytes.is_empty() {
        return Err(ElevenLabsTtsError::EmptyAudio);
    }
    Ok(ElevenLabsTtsResult {
        audio_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        content_type,
        model: intent.model,
        voice_id: intent.voice_id,
        latency_ms: started.elapsed().as_millis(),
    })
}

#[derive(Debug, PartialEq)]
struct NormalizedTtsIntent {
    text: String,
    voice_id: String,
    model: String,
}

impl NormalizedTtsIntent {
    fn from_intent(
        intent: ElevenLabsTtsIntent,
        settings_model: &str,
    ) -> Result<Self, ElevenLabsTtsError> {
        let text = intent.text.trim().to_owned();
        if text.is_empty() {
            return Err(ElevenLabsTtsError::InvalidRequest(
                "text is empty".to_owned(),
            ));
        }
        let voice_id = intent.voice_id.trim().to_owned();
        if voice_id.is_empty() {
            return Err(ElevenLabsTtsError::InvalidRequest(
                "voice_id is empty".to_owned(),
            ));
        }
        let model = intent
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .or_else(|| {
                let model = settings_model.trim();
                if model.is_empty() {
                    None
                } else {
                    Some(model)
                }
            })
            .unwrap_or(DEFAULT_TTS_MODEL)
            .to_owned();
        Ok(Self {
            text,
            voice_id,
            model,
        })
    }
}

#[derive(Serialize)]
struct TtsRequestBody<'a> {
    text: &'a str,
    model_id: &'a str,
    voice_settings: TtsVoiceSettings,
}

#[derive(Serialize)]
struct TtsVoiceSettings {
    stability: f64,
    similarity_boost: f64,
}

fn tts_url(voice_id: &str) -> Result<Url, ElevenLabsTtsError> {
    let mut url = Url::parse(ELEVENLABS_BASE_URL)
        .map_err(|e| ElevenLabsTtsError::InvalidRequest(format!("invalid base URL: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| ElevenLabsTtsError::InvalidRequest("invalid base URL".to_owned()))?
        .push("v1")
        .push("text-to-speech")
        .push(voice_id);
    Ok(url)
}

fn map_reqwest_error(error: reqwest::Error) -> ElevenLabsTtsError {
    if error.is_timeout() {
        ElevenLabsTtsError::Transport("request timed out".to_owned())
    } else {
        ElevenLabsTtsError::Transport(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_tts_intent() {
        let normalized = NormalizedTtsIntent::from_intent(
            ElevenLabsTtsIntent {
                text: "  hello  ".to_owned(),
                voice_id: " voice-a ".to_owned(),
                model: None,
            },
            " eleven_multilingual_v2 ",
        )
        .unwrap();

        assert_eq!(
            normalized,
            NormalizedTtsIntent {
                text: "hello".to_owned(),
                voice_id: "voice-a".to_owned(),
                model: "eleven_multilingual_v2".to_owned(),
            }
        );
    }

    #[test]
    fn request_model_overrides_settings_model() {
        let normalized = NormalizedTtsIntent::from_intent(
            ElevenLabsTtsIntent {
                text: "hello".to_owned(),
                voice_id: "voice-a".to_owned(),
                model: Some("eleven_flash_v2_5".to_owned()),
            },
            "eleven_multilingual_v2",
        )
        .unwrap();

        assert_eq!(normalized.model, "eleven_flash_v2_5");
    }

    #[test]
    fn empty_text_uses_stable_ffi_kind() {
        let error = NormalizedTtsIntent::from_intent(
            ElevenLabsTtsIntent {
                text: " ".to_owned(),
                voice_id: "voice-a".to_owned(),
                model: None,
            },
            "",
        )
        .unwrap_err();

        assert_eq!(error.kind(), "invalid_request");
    }

    #[test]
    fn tts_url_escapes_voice_path_segment() {
        let url = tts_url("voice/a").unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.elevenlabs.io/v1/text-to-speech/voice%2Fa"
        );
    }
}
