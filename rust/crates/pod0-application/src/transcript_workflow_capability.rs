use pod0_domain::{
    EpisodeId, TranscriptArtifactInput, TranscriptAttemptId, TranscriptSubmissionFenceId,
};

use crate::{
    MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES, MAX_TRANSCRIPT_EXTERNAL_ID_BYTES,
    MAX_TRANSCRIPT_MODEL_BYTES, MAX_TRANSCRIPT_PROVIDER_STATUS_BYTES,
    MAX_TRANSCRIPT_SAFE_DETAIL_BYTES, TranscriptFailureEvidence, TranscriptProvider,
    TranscriptWorkflowFailureCode,
};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptCapabilityRequest {
    FetchPublisher {
        episode_id: EpisodeId,
        source_url: String,
        mime_hint: Option<String>,
        maximum_response_bytes: u64,
    },
    SubmitProvider {
        episode_id: EpisodeId,
        attempt_id: TranscriptAttemptId,
        submission_fence_id: TranscriptSubmissionFenceId,
        provider: TranscriptProvider,
        model: String,
        audio_url: String,
        maximum_response_bytes: u64,
    },
    RecoverProvider {
        episode_id: EpisodeId,
        attempt_id: TranscriptAttemptId,
        submission_fence_id: TranscriptSubmissionFenceId,
        provider: TranscriptProvider,
        model: String,
        external_operation_id: String,
        provider_status: Option<String>,
        maximum_response_bytes: u64,
    },
    TranscribeLocal {
        episode_id: EpisodeId,
        attempt_id: TranscriptAttemptId,
        audio_url: String,
        locale: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptCapabilityObservation {
    ProviderAccepted {
        external_operation_id: String,
        provider_status: Option<String>,
    },
    ProviderPending {
        provider_status: Option<String>,
        retry_after_milliseconds: Option<u64>,
    },
    Completed {
        artifact: TranscriptArtifactInput,
    },
    Failed {
        evidence: TranscriptFailureEvidence,
        safe_detail: Option<String>,
        retry_after_milliseconds: Option<u64>,
    },
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptCapabilityValidation {
    Accepted,
    Rejected { code: TranscriptWorkflowFailureCode },
}

#[must_use]
pub fn validate_transcript_capability_request(
    request: TranscriptCapabilityRequest,
) -> TranscriptCapabilityValidation {
    use TranscriptCapabilityRequest as Request;
    match request {
        Request::FetchPublisher {
            source_url,
            mime_hint,
            maximum_response_bytes,
            ..
        } => {
            if !valid_remote_url(&source_url)
                || invalid_text(mime_hint.as_deref(), 128)
                || invalid_response_bound(maximum_response_bytes)
            {
                rejected(TranscriptWorkflowFailureCode::InvalidRequest)
            } else {
                TranscriptCapabilityValidation::Accepted
            }
        }
        Request::SubmitProvider {
            provider,
            model,
            audio_url,
            maximum_response_bytes,
            ..
        } => {
            if crate::normalize_media_url(&audio_url).is_none() {
                rejected(TranscriptWorkflowFailureCode::InvalidRequest)
            } else {
                validate_provider_identity(provider, &model, maximum_response_bytes)
            }
        }
        Request::RecoverProvider {
            provider,
            model,
            external_operation_id,
            provider_status,
            maximum_response_bytes,
            ..
        } => {
            if external_operation_id.trim().is_empty()
                || external_operation_id.len() > MAX_TRANSCRIPT_EXTERNAL_ID_BYTES
                || invalid_text(
                    provider_status.as_deref(),
                    MAX_TRANSCRIPT_PROVIDER_STATUS_BYTES,
                )
            {
                return rejected(TranscriptWorkflowFailureCode::InvalidRequest);
            }
            validate_provider_identity(provider, &model, maximum_response_bytes)
        }
        Request::TranscribeLocal {
            audio_url, locale, ..
        } => {
            if !audio_url.starts_with("file://")
                || crate::normalize_media_url(&audio_url).is_none()
                || invalid_text(locale.as_deref(), 64)
            {
                rejected(TranscriptWorkflowFailureCode::InvalidRequest)
            } else {
                TranscriptCapabilityValidation::Accepted
            }
        }
    }
}

#[must_use]
pub fn validate_transcript_capability_observation(
    observation: TranscriptCapabilityObservation,
) -> TranscriptCapabilityValidation {
    use TranscriptCapabilityObservation as Observation;
    match observation {
        Observation::ProviderAccepted {
            external_operation_id,
            provider_status,
        } => {
            if external_operation_id.trim().is_empty()
                || external_operation_id.len() > MAX_TRANSCRIPT_EXTERNAL_ID_BYTES
                || invalid_text(
                    provider_status.as_deref(),
                    MAX_TRANSCRIPT_PROVIDER_STATUS_BYTES,
                )
            {
                rejected(TranscriptWorkflowFailureCode::InvalidResponse)
            } else {
                TranscriptCapabilityValidation::Accepted
            }
        }
        Observation::ProviderPending {
            provider_status, ..
        } => {
            if invalid_text(
                provider_status.as_deref(),
                MAX_TRANSCRIPT_PROVIDER_STATUS_BYTES,
            ) {
                rejected(TranscriptWorkflowFailureCode::InvalidResponse)
            } else {
                TranscriptCapabilityValidation::Accepted
            }
        }
        Observation::Completed { artifact } => {
            if pod0_domain::TranscriptArtifact::seal(artifact).is_ok() {
                TranscriptCapabilityValidation::Accepted
            } else {
                rejected(TranscriptWorkflowFailureCode::InvalidResponse)
            }
        }
        Observation::Failed { safe_detail, .. } => {
            if invalid_text(safe_detail.as_deref(), MAX_TRANSCRIPT_SAFE_DETAIL_BYTES) {
                rejected(TranscriptWorkflowFailureCode::InvalidResponse)
            } else {
                TranscriptCapabilityValidation::Accepted
            }
        }
        Observation::Cancelled => TranscriptCapabilityValidation::Accepted,
    }
}

fn validate_provider_identity(
    provider: TranscriptProvider,
    model: &str,
    maximum_response_bytes: u64,
) -> TranscriptCapabilityValidation {
    if matches!(
        provider,
        TranscriptProvider::AppleSpeech | TranscriptProvider::Unsupported { .. }
    ) {
        return rejected(TranscriptWorkflowFailureCode::UnsupportedProvider);
    }
    if model.trim().is_empty()
        || model.trim() != model
        || model.len() > MAX_TRANSCRIPT_MODEL_BYTES
        || invalid_response_bound(maximum_response_bytes)
    {
        rejected(TranscriptWorkflowFailureCode::InvalidRequest)
    } else {
        TranscriptCapabilityValidation::Accepted
    }
}

fn invalid_response_bound(value: u64) -> bool {
    value == 0 || value > MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES
}

fn invalid_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_some_and(|text| text.trim() != text || text.len() > maximum_bytes)
}

fn valid_remote_url(value: &str) -> bool {
    crate::normalize_media_url(value).is_some()
        && (value.starts_with("https://") || value.starts_with("http://"))
}

const fn rejected(code: TranscriptWorkflowFailureCode) -> TranscriptCapabilityValidation {
    TranscriptCapabilityValidation::Rejected { code }
}
