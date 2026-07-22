use pod0_application::{
    HostObservationReceipt, HostObservationRejection,
    TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS, TranscriptFailureClassification,
    TranscriptRetryDisposition, TranscriptWorkflowFailureCode, transcript_attempt_id,
    transcript_retry_not_before, transcript_submission_fence_id,
};
use pod0_storage::{
    PreparedTranscriptAttempt, StorageError, TranscriptWorkflowFailureDisposition,
    TranscriptWorkflowRecord,
};

use crate::runtime_chapter_model_receipts::{rejected, retain};
use crate::runtime_transcript_workflow_mapping::request_id;

pub(super) fn failure_disposition(
    record: &TranscriptWorkflowRecord,
    classification: TranscriptFailureClassification,
    issued_revision: pod0_domain::StateRevision,
    now_ms: i64,
    retry_after_milliseconds: Option<u64>,
) -> TranscriptWorkflowFailureDisposition {
    if classification.retry == TranscriptRetryDisposition::AutomaticRequest
        && classification.resubmission_is_safe
        && record.attempt < record.max_attempts
    {
        let attempt_number = record.attempt.saturating_add(1).max(1);
        if let Some(attempt_id) = transcript_attempt_id(record.request.workflow_id, attempt_number)
        {
            let not_before = transcript_retry_not_before(
                pod0_domain::UnixTimestampMilliseconds::new(now_ms),
                attempt_number,
                retry_after_milliseconds.and_then(|value| i64::try_from(value).ok()),
            )
            .value;
            return TranscriptWorkflowFailureDisposition::Retry {
                attempt: PreparedTranscriptAttempt {
                    attempt: attempt_number,
                    attempt_id,
                    submission_fence_id: transcript_submission_fence_id(attempt_id),
                },
                request_id: request_id(record.request.workflow_id, attempt_number, false),
                issued_revision,
                not_before_ms: not_before,
                deadline_at_ms: not_before
                    .saturating_add(TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS),
                evidence_permits_resubmission: true,
            };
        }
    }
    match classification.retry {
        TranscriptRetryDisposition::RecoverPersisted => {
            TranscriptWorkflowFailureDisposition::RecoverPersisted
        }
        TranscriptRetryDisposition::Replan => TranscriptWorkflowFailureDisposition::Block,
        TranscriptRetryDisposition::ExplicitOnly if classification.may_have_submitted => {
            TranscriptWorkflowFailureDisposition::Ambiguous
        }
        _ if classification.code == TranscriptWorkflowFailureCode::Cancelled => {
            TranscriptWorkflowFailureDisposition::Cancel
        }
        _ => TranscriptWorkflowFailureDisposition::Fail,
    }
}

pub(super) const fn failure_wire(code: TranscriptWorkflowFailureCode) -> &'static str {
    use TranscriptWorkflowFailureCode as Code;
    match code {
        Code::MissingCredential => "missing_credential",
        Code::MissingLocalAudio => "missing_local_audio",
        Code::InvalidRequest => "invalid_request",
        Code::UnsupportedProvider => "unsupported_provider",
        Code::PublisherUnavailable => "publisher_unavailable",
        Code::Offline => "offline",
        Code::RateLimited => "rate_limited",
        Code::TimedOut => "timed_out",
        Code::Transport => "transport",
        Code::PermissionDenied => "permission_denied",
        Code::ProviderRejected => "provider_rejected",
        Code::ProviderUnavailable => "provider_unavailable",
        Code::ResponseTooLarge => "response_too_large",
        Code::InvalidResponse => "invalid_response",
        Code::StaleInput => "stale_input",
        Code::StorageUnavailable => "storage_unavailable",
        Code::AmbiguousSubmission => "ambiguous_submission",
        Code::ProviderRecoveryUnavailable => "provider_recovery_unavailable",
        Code::RetryExhausted => "retry_exhausted",
        Code::Cancelled => "cancelled",
        Code::Unsupported { .. } => "unsupported",
    }
}

pub(super) fn storage_receipt(
    request_id: pod0_domain::HostRequestId,
    error: StorageError,
) -> HostObservationReceipt {
    match error {
        StorageError::TranscriptWorkflowConflict
        | StorageError::TranscriptWorkflowNotFound
        | StorageError::StaleTranscriptAttempt => {
            rejected(request_id, HostObservationRejection::StaleWorkflow)
        }
        _ => retain(request_id),
    }
}
