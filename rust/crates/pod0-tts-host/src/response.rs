use reqwest::{
    Client,
    header::{ACCEPT, ACCEPT_ENCODING, HeaderMap, HeaderValue},
};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncWriteExt as _;

use crate::{
    AudioMediaType, CredentialError, CredentialErrorKind, FileOperation, GenerationRequest,
    LimitError, LimitSubject, ProtocolError, ProtocolErrorKind, ProviderError, TtsError, audio,
};

const MAX_RESPONSE_METADATA_BYTES: usize = 1_024;
const MAX_ERROR_BODY_BYTES: u64 = 8 * 1_024;

pub(crate) struct Downloaded {
    pub status: u16,
    pub request_id: Option<String>,
    pub media_type: AudioMediaType,
    pub byte_count: u64,
    pub content_digest: [u8; 32],
}

pub(crate) async fn download(
    client: &Client,
    request: &GenerationRequest<'_>,
    file: &mut tokio::fs::File,
) -> Result<Downloaded, TtsError> {
    let url = request
        .endpoint
        .generation_url(request.voice_id, request.output_format)?;
    let mut secret_header = HeaderValue::from_str(request.secret.expose()).map_err(|_| {
        TtsError::Credential(CredentialError {
            kind: CredentialErrorKind::InvalidHeaderValue,
        })
    })?;
    secret_header.set_sensitive(true);
    let response = client
        .post(url)
        .header("xi-api-key", secret_header)
        .header(ACCEPT, "audio/*")
        .header(ACCEPT_ENCODING, "identity")
        .json(&ProviderBody {
            text: request.script,
            model_id: request.model_id,
        })
        .send()
        .await
        .map_err(|error| TtsError::from_reqwest(&error))?;
    let status = response.status().as_u16();
    let request_id = request_id(response.headers(), request.secret.expose(), status)?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(provider_error(response, status).await?);
    }
    let media_type = content_type(response.headers(), request.output_format, status)?;
    if response.content_length() == Some(0) {
        return Err(protocol(ProtocolErrorKind::EmptyAudio, status));
    }
    if response
        .content_length()
        .is_some_and(|length| length > request.limits.maximum_output_bytes)
    {
        return Err(limit(
            LimitSubject::Output,
            request.limits.maximum_output_bytes,
            response.content_length(),
        ));
    }
    stream_audio(response, file, request, status, request_id, media_type).await
}

async fn stream_audio(
    mut response: reqwest::Response,
    file: &mut tokio::fs::File,
    request: &GenerationRequest<'_>,
    status: u16,
    request_id: Option<String>,
    media_type: AudioMediaType,
) -> Result<Downloaded, TtsError> {
    let mut byte_count = 0_u64;
    let mut hasher = Sha256::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| TtsError::from_reqwest(&error))?
    {
        byte_count = byte_count
            .checked_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| {
                limit(
                    LimitSubject::Output,
                    request.limits.maximum_output_bytes,
                    None,
                )
            })?;
        if byte_count > request.limits.maximum_output_bytes {
            return Err(limit(
                LimitSubject::Output,
                request.limits.maximum_output_bytes,
                Some(byte_count),
            ));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| TtsError::from_io(FileOperation::WriteTemporary, &error))?;
        hasher.update(&chunk);
    }
    if byte_count == 0 {
        return Err(protocol(ProtocolErrorKind::EmptyAudio, status));
    }
    file.flush()
        .await
        .map_err(|error| TtsError::from_io(FileOperation::FlushTemporary, &error))?;
    audio::validate(file, media_type, byte_count, status).await?;
    file.sync_all()
        .await
        .map_err(|error| TtsError::from_io(FileOperation::SyncTemporary, &error))?;
    Ok(Downloaded {
        status,
        request_id,
        media_type,
        byte_count,
        content_digest: hasher.finalize().into(),
    })
}

#[derive(Serialize)]
struct ProviderBody<'a> {
    text: &'a str,
    model_id: &'a str,
}

fn content_type(
    headers: &HeaderMap,
    output_format: Option<&str>,
    status: u16,
) -> Result<AudioMediaType, TtsError> {
    let raw = headers
        .get(reqwest::header::CONTENT_TYPE)
        .ok_or_else(|| protocol(ProtocolErrorKind::MissingContentType, status))?
        .to_str()
        .map_err(|_| protocol(ProtocolErrorKind::InvalidContentType, status))?;
    let normalized = raw
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(protocol(ProtocolErrorKind::MissingContentType, status));
    }
    let declared = AudioMediaType::parse(&normalized)
        .ok_or_else(|| protocol(ProtocolErrorKind::UnsupportedContentType, status))?;
    let expected = AudioMediaType::for_output_format(output_format)
        .ok_or_else(|| protocol(ProtocolErrorKind::MismatchedContentType, status))?;
    if !expected.accepts_declaration(declared) {
        return Err(protocol(ProtocolErrorKind::MismatchedContentType, status));
    }
    Ok(expected)
}

fn request_id(headers: &HeaderMap, secret: &str, status: u16) -> Result<Option<String>, TtsError> {
    for name in ["request-id", "x-request-id", "x-correlation-id"] {
        let Some(value) = headers.get(name) else {
            continue;
        };
        if value.as_bytes().len() > MAX_RESPONSE_METADATA_BYTES {
            return Err(limit(
                LimitSubject::Metadata,
                MAX_RESPONSE_METADATA_BYTES as u64,
                Some(value.as_bytes().len() as u64),
            ));
        }
        let value = value
            .to_str()
            .map_err(|_| protocol(ProtocolErrorKind::InvalidMetadata, status))?;
        if !value.contains(secret) {
            return Ok(Some(value.to_owned()));
        }
    }
    Ok(None)
}

async fn provider_error(
    mut response: reqwest::Response,
    status: u16,
) -> Result<TtsError, TtsError> {
    let mut body_bytes = 0_u64;
    let mut body_truncated = response
        .content_length()
        .is_some_and(|length| length > MAX_ERROR_BODY_BYTES);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| TtsError::from_reqwest(&error))?
    {
        body_bytes = body_bytes.saturating_add(chunk.len() as u64);
        if body_bytes > MAX_ERROR_BODY_BYTES {
            body_truncated = true;
            break;
        }
    }
    Ok(TtsError::Provider(ProviderError {
        status,
        response_body_bytes: body_bytes.min(MAX_ERROR_BODY_BYTES),
        body_truncated,
    }))
}

fn limit(subject: LimitSubject, limit: u64, observed: Option<u64>) -> TtsError {
    TtsError::Limit(LimitError {
        subject,
        limit,
        observed,
    })
}

fn protocol(kind: ProtocolErrorKind, status: u16) -> TtsError {
    TtsError::Protocol(ProtocolError {
        kind,
        status: Some(status),
    })
}
