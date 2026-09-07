use crate::{
    AudioMediaType, CredentialError, CredentialErrorKind, GenerationRequest, InvalidRequestError,
    InvalidRequestField, InvalidRequestReason, LimitError, LimitSubject, TtsError,
};

const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_OUTPUT_FORMAT_BYTES: usize = 64;

pub(crate) fn validate(request: &GenerationRequest<'_>) -> Result<(), TtsError> {
    if request.secret.expose().is_empty() {
        return Err(TtsError::Credential(CredentialError {
            kind: CredentialErrorKind::Missing,
        }));
    }
    if request.timeout.is_zero() {
        return Err(invalid(
            InvalidRequestField::Timeout,
            InvalidRequestReason::Zero,
        ));
    }
    if request.limits.maximum_script_bytes == 0 || request.limits.maximum_output_bytes == 0 {
        return Err(invalid(
            InvalidRequestField::Limits,
            InvalidRequestReason::Zero,
        ));
    }
    if request.script.trim().is_empty() {
        return Err(invalid(
            InvalidRequestField::Script,
            InvalidRequestReason::Empty,
        ));
    }
    if request.script.len() > request.limits.maximum_script_bytes {
        return Err(limit(
            LimitSubject::Script,
            request.limits.maximum_script_bytes as u64,
            Some(request.script.len() as u64),
        ));
    }
    validate_identifier(
        request.model_id,
        InvalidRequestField::ModelId,
        LimitSubject::ModelId,
    )?;
    validate_identifier(
        request.voice_id,
        InvalidRequestField::VoiceId,
        LimitSubject::VoiceId,
    )?;
    if let Some(format) = request.output_format {
        if format.is_empty()
            || !format
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(invalid(
                InvalidRequestField::OutputFormat,
                InvalidRequestReason::Invalid,
            ));
        }
        if format.len() > MAX_OUTPUT_FORMAT_BYTES {
            return Err(limit(
                LimitSubject::OutputFormat,
                MAX_OUTPUT_FORMAT_BYTES as u64,
                Some(format.len() as u64),
            ));
        }
        if AudioMediaType::for_output_format(Some(format)).is_none() {
            return Err(invalid(
                InvalidRequestField::OutputFormat,
                InvalidRequestReason::Invalid,
            ));
        }
    }
    Ok(())
}

fn validate_identifier(
    value: &str,
    field: InvalidRequestField,
    subject: LimitSubject,
) -> Result<(), TtsError> {
    if value.is_empty() {
        return Err(invalid(field, InvalidRequestReason::Empty));
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(limit(
            subject,
            MAX_IDENTIFIER_BYTES as u64,
            Some(value.len() as u64),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(field, InvalidRequestReason::Invalid));
    }
    Ok(())
}

fn invalid(field: InvalidRequestField, reason: InvalidRequestReason) -> TtsError {
    TtsError::InvalidRequest(InvalidRequestError { field, reason })
}

fn limit(subject: LimitSubject, limit: u64, observed: Option<u64>) -> TtsError {
    TtsError::Limit(LimitError {
        subject,
        limit,
        observed,
    })
}
