use pod0_application::{
    TranscriptCapabilityObservation, TranscriptFailureTransition, TranscriptObservationDecision,
    TranscriptWorkflowFailureCode,
};

use crate::{
    PreparedTranscriptAttempt, StorageError, TranscriptCompletionInput,
    TranscriptProviderAcceptedInput, TranscriptProviderPendingInput,
    TranscriptWorkflowFailureDisposition, TranscriptWorkflowFailureInput,
};

pub(super) fn apply_observation(
    transaction: &rusqlite::Transaction<'_>,
    current: crate::TranscriptWorkflowRecord,
    durable: pod0_application::DurableTranscriptHostObservation,
    decision: TranscriptObservationDecision,
    committed_at_ms: i64,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let request_id = durable.request_id;
    match (durable.observation, decision) {
        (
            TranscriptCapabilityObservation::ProviderAccepted {
                external_operation_id,
                provider_status,
            },
            TranscriptObservationDecision::ProviderAccepted { not_before },
        ) => crate::transcript_workflow::apply_transcript_provider_accepted(
            transaction,
            TranscriptProviderAcceptedInput {
                episode_id: current.episode_id,
                request_id,
                attempt_id: current
                    .attempt_id
                    .ok_or(StorageError::StaleTranscriptAttempt)?,
                submission_fence_id: current
                    .submission_fence_id
                    .ok_or(StorageError::StaleTranscriptAttempt)?,
                external_operation_id,
                provider_status,
                observed_at_ms: committed_at_ms,
            },
            not_before.value,
        ),
        (
            TranscriptCapabilityObservation::ProviderPending {
                provider_status, ..
            },
            TranscriptObservationDecision::ProviderPending { not_before },
        ) => crate::transcript_workflow::apply_transcript_provider_pending(
            transaction,
            TranscriptProviderPendingInput {
                episode_id: current.episode_id,
                request_id,
                attempt_id: current
                    .attempt_id
                    .ok_or(StorageError::StaleTranscriptAttempt)?,
                submission_fence_id: current
                    .submission_fence_id
                    .ok_or(StorageError::StaleTranscriptAttempt)?,
                provider_status,
                not_before_ms: not_before.value,
                observed_at_ms: committed_at_ms,
            },
        ),
        (
            TranscriptCapabilityObservation::Completed {
                external_operation_id,
                provider_status,
                artifact,
            },
            TranscriptObservationDecision::Completion,
        ) => crate::transcript_workflow::apply_transcript_completion(
            transaction,
            TranscriptCompletionInput {
                episode_id: current.episode_id,
                request_id,
                attempt_id: current.attempt_id,
                submission_fence_id: current.submission_fence_id,
                external_operation_id,
                provider_status,
                artifact,
                observed_at_ms: committed_at_ms,
            },
        ),
        (
            TranscriptCapabilityObservation::Failed { safe_detail, .. },
            TranscriptObservationDecision::Failure {
                code,
                safe_detail: decision_detail,
                retryable,
                may_have_submitted,
                transition,
            },
        ) if safe_detail == decision_detail => {
            crate::transcript_workflow::apply_transcript_failure(
                transaction,
                failure_input(
                    &current,
                    request_id,
                    code,
                    decision_detail,
                    retryable,
                    may_have_submitted,
                    transition,
                    committed_at_ms,
                ),
            )
        }
        (
            TranscriptCapabilityObservation::Cancelled,
            TranscriptObservationDecision::Failure {
                code,
                safe_detail: None,
                retryable,
                may_have_submitted,
                transition,
            },
        ) => crate::transcript_workflow::apply_transcript_failure(
            transaction,
            failure_input(
                &current,
                request_id,
                code,
                None,
                retryable,
                may_have_submitted,
                transition,
                committed_at_ms,
            ),
        ),
        _ => Err(StorageError::InvalidActivity),
    }
}

#[allow(clippy::too_many_arguments)]
fn failure_input(
    current: &crate::TranscriptWorkflowRecord,
    request_id: pod0_domain::HostRequestId,
    code: TranscriptWorkflowFailureCode,
    detail: Option<String>,
    retryable: bool,
    may_have_submitted: bool,
    transition: TranscriptFailureTransition,
    observed_at_ms: i64,
) -> TranscriptWorkflowFailureInput {
    TranscriptWorkflowFailureInput {
        episode_id: current.episode_id,
        request_id,
        attempt_id: current.attempt_id,
        submission_fence_id: current.submission_fence_id,
        failure_code: failure_wire(code).to_owned(),
        failure_detail: detail,
        retryable,
        may_have_submitted,
        disposition: storage_failure_transition(transition),
        observed_at_ms,
    }
}

fn storage_failure_transition(
    transition: TranscriptFailureTransition,
) -> TranscriptWorkflowFailureDisposition {
    match transition {
        TranscriptFailureTransition::Retry {
            attempt,
            attempt_id,
            submission_fence_id,
            request_id,
            issued_revision,
            not_before,
            deadline_at,
            evidence_permits_resubmission,
        } => TranscriptWorkflowFailureDisposition::Retry {
            attempt: PreparedTranscriptAttempt {
                attempt,
                attempt_id,
                submission_fence_id,
            },
            request_id,
            issued_revision,
            not_before_ms: not_before.value,
            deadline_at_ms: deadline_at.value,
            evidence_permits_resubmission,
        },
        TranscriptFailureTransition::RecoverPersisted => {
            TranscriptWorkflowFailureDisposition::RecoverPersisted
        }
        TranscriptFailureTransition::Block => TranscriptWorkflowFailureDisposition::Block,
        TranscriptFailureTransition::Fail => TranscriptWorkflowFailureDisposition::Fail,
        TranscriptFailureTransition::Ambiguous => TranscriptWorkflowFailureDisposition::Ambiguous,
        TranscriptFailureTransition::Cancel => TranscriptWorkflowFailureDisposition::Cancel,
    }
}

const fn failure_wire(code: TranscriptWorkflowFailureCode) -> &'static str {
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
