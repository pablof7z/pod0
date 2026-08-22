use std::{path::PathBuf, time::Duration};

use reqwest::multipart::Form;
use serde_json::Value;

use crate::{
    AdapterError, CancellationToken, HttpEvidence, HttpLimits, LiveHosts, ProviderEndpoint,
    ProviderKind,
    bounds::{bounded_body, enforce_output},
    provider::invalid_provider_response,
    transcription_upload::audio_part,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptionResponseFormat {
    Json,
    VerboseJson,
}

impl TranscriptionResponseFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::VerboseJson => "verbose_json",
        }
    }
}

#[derive(Debug)]
pub struct TranscriptionRequest {
    pub endpoint: ProviderEndpoint,
    pub model: String,
    pub audio_path: PathBuf,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub temperature: Option<f64>,
    pub response_format: TranscriptionResponseFormat,
    pub timeout: Duration,
    pub limits: HttpLimits,
    pub maximum_upload_bytes: u64,
    pub maximum_output_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptionResponse {
    pub text: String,
    pub language: Option<String>,
    pub duration_seconds: Option<f64>,
    pub segments: Option<Value>,
    pub evidence: HttpEvidence,
}

impl LiveHosts {
    pub async fn transcribe_audio(
        &self,
        request: TranscriptionRequest,
        cancellation: &CancellationToken,
    ) -> Result<TranscriptionResponse, AdapterError> {
        cancellation.check()?;
        request.limits.validate()?;
        validate_request(&request)?;
        self.run(request.timeout, cancellation, async {
            let file = audio_part(&request.audio_path, request.maximum_upload_bytes).await?;
            let mut form = Form::new()
                .part("file", file)
                .text("model", request.model.clone())
                .text(
                    "response_format",
                    request.response_format.as_str().to_owned(),
                );
            if let Some(language) = &request.language {
                form = form.text("language", language.clone());
            }
            if let Some(prompt) = &request.prompt {
                form = form.text("prompt", prompt.clone());
            }
            if let Some(temperature) = request.temperature {
                form = form.text("temperature", temperature.to_string());
            }
            let response = self
                .provider_request(&request.endpoint, ProviderKind::Transcription)?
                .multipart(form)
                .send()
                .await
                .map_err(|error| AdapterError::from_reqwest(&error))?;
            let (response, evidence) = self
                .provider_response(response, ProviderKind::Transcription, request.limits)
                .await?;
            let bytes = bounded_body(response, request.limits.maximum_body_bytes).await?;
            parse_response(&bytes, evidence, request.maximum_output_bytes)
        })
        .await
    }
}

fn validate_request(request: &TranscriptionRequest) -> Result<(), AdapterError> {
    if request.model.trim().is_empty()
        || request.maximum_upload_bytes == 0
        || request.maximum_output_bytes == 0
        || request.temperature.is_some_and(|value| !value.is_finite())
    {
        return Err(invalid_provider_response("transcription request"));
    }
    Ok(())
}

fn parse_response(
    bytes: &[u8],
    evidence: HttpEvidence,
    maximum_output_bytes: u64,
) -> Result<TranscriptionResponse, AdapterError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| invalid_provider_response("transcription JSON"))?;
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_provider_response("transcription text"))?
        .to_owned();
    let language = optional_string(&value, "language")?;
    let duration_seconds = match value.get("duration") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_f64()
                .filter(|duration| duration.is_finite() && *duration >= 0.0)
                .ok_or_else(|| invalid_provider_response("transcription duration"))?,
        ),
    };
    let segments = value.get("segments").cloned();
    let segment_bytes = segments
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|_| invalid_provider_response("transcription segments"))?
        .map_or(0, |value| value.len());
    enforce_output(
        text.len() + language.as_deref().map_or(0, str::len) + segment_bytes,
        maximum_output_bytes,
    )?;
    Ok(TranscriptionResponse {
        text,
        language,
        duration_seconds,
        segments,
        evidence,
    })
}

fn optional_string(value: &Value, key: &'static str) -> Result<Option<String>, AdapterError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(invalid_provider_response(key)),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn transcription_text_obeys_output_bound() {
        let evidence = HttpEvidence {
            status: 200,
            final_url: "https://example.invalid/transcriptions".to_owned(),
            redirects: Vec::new(),
            entity_tag: None,
            last_modified: None,
            content_type: None,
            content_length: None,
        };
        assert!(matches!(
            parse_response(br#"{"text":"too long"}"#, evidence, 3),
            Err(AdapterError::Size(_))
        ));
    }

    #[tokio::test]
    async fn pre_cancelled_transcription_does_not_open_audio() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = LiveHosts::default()
            .transcribe_audio(
                TranscriptionRequest {
                    endpoint: ProviderEndpoint::unauthenticated("http://127.0.0.1:1"),
                    model: "model".to_owned(),
                    audio_path: PathBuf::from("missing-audio"),
                    language: None,
                    prompt: None,
                    temperature: None,
                    response_format: TranscriptionResponseFormat::Json,
                    timeout: Duration::from_secs(1),
                    limits: HttpLimits {
                        maximum_body_bytes: 1,
                        maximum_metadata_bytes: 1,
                    },
                    maximum_upload_bytes: 1,
                    maximum_output_bytes: 1,
                },
                &cancellation,
            )
            .await
            .expect_err("pre-cancelled transcription must not inspect the file");
        assert!(matches!(error, AdapterError::Cancelled));
    }
}
