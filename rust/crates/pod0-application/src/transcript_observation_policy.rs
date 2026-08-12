use pod0_domain::{
    HostRequestId, StateRevision, TranscriptAttemptId, TranscriptSubmissionFenceId,
    TranscriptWorkflowId, UnixTimestampMilliseconds,
};

use crate::{
    TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS, TRANSCRIPT_RETRY_BASE_MILLISECONDS,
    TranscriptCapabilityObservation, TranscriptFailureEvidence, TranscriptRetryDisposition,
    TranscriptWorkflowFailureCode, classify_transcript_failure, transcript_attempt_id,
    transcript_retry_not_before, transcript_submission_fence_id, transcript_workflow_request_id,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptObservationPolicyState {
    pub workflow_id: TranscriptWorkflowId,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub max_attempts: u16,
    pub submission_authorized: bool,
    pub provider_accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptObservationPolicyInput {
    pub state: TranscriptObservationPolicyState,
    pub observation: TranscriptCapabilityObservation,
    pub observed_at: UnixTimestampMilliseconds,
    pub retry_issued_revision: StateRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptObservationDecision {
    ProviderAccepted {
        not_before: UnixTimestampMilliseconds,
    },
    ProviderPending {
        not_before: UnixTimestampMilliseconds,
    },
    Completion,
    Failure {
        code: TranscriptWorkflowFailureCode,
        safe_detail: Option<String>,
        retryable: bool,
        may_have_submitted: bool,
        transition: TranscriptFailureTransition,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptFailureTransition {
    Retry {
        attempt: u16,
        attempt_id: TranscriptAttemptId,
        submission_fence_id: TranscriptSubmissionFenceId,
        request_id: HostRequestId,
        issued_revision: StateRevision,
        not_before: UnixTimestampMilliseconds,
        deadline_at: UnixTimestampMilliseconds,
        evidence_permits_resubmission: bool,
    },
    RecoverPersisted,
    Block,
    Fail,
    Ambiguous,
    Cancel,
}

#[must_use]
pub fn decide_transcript_observation(
    input: TranscriptObservationPolicyInput,
) -> TranscriptObservationDecision {
    match input.observation {
        TranscriptCapabilityObservation::ProviderAccepted { .. } => {
            TranscriptObservationDecision::ProviderAccepted {
                not_before: UnixTimestampMilliseconds::new(
                    input
                        .observed_at
                        .value
                        .saturating_add(TRANSCRIPT_RETRY_BASE_MILLISECONDS),
                ),
            }
        }
        TranscriptCapabilityObservation::ProviderPending {
            retry_after_milliseconds,
            ..
        } => TranscriptObservationDecision::ProviderPending {
            not_before: UnixTimestampMilliseconds::new(
                input
                    .observed_at
                    .value
                    .saturating_add(bounded_provider_delay(retry_after_milliseconds)),
            ),
        },
        TranscriptCapabilityObservation::Completed { .. } => {
            TranscriptObservationDecision::Completion
        }
        TranscriptCapabilityObservation::Failed {
            evidence,
            safe_detail,
            retry_after_milliseconds,
        } => failure_decision(
            input.state,
            evidence,
            safe_detail,
            retry_after_milliseconds,
            input.observed_at,
            input.retry_issued_revision,
        ),
        TranscriptCapabilityObservation::Cancelled => failure_decision(
            input.state,
            TranscriptFailureEvidence::Cancelled {
                submission_authorized: input.state.submission_authorized,
                provider_accepted: input.state.provider_accepted,
            },
            None,
            None,
            input.observed_at,
            input.retry_issued_revision,
        ),
    }
}

fn failure_decision(
    state: TranscriptObservationPolicyState,
    evidence: TranscriptFailureEvidence,
    safe_detail: Option<String>,
    retry_after_milliseconds: Option<u64>,
    observed_at: UnixTimestampMilliseconds,
    retry_issued_revision: StateRevision,
) -> TranscriptObservationDecision {
    let classification = classify_transcript_failure(evidence);
    let transition = retry_transition(
        state,
        classification,
        retry_after_milliseconds,
        observed_at,
        retry_issued_revision,
    );
    TranscriptObservationDecision::Failure {
        code: classification.code,
        safe_detail,
        retryable: !matches!(classification.retry, TranscriptRetryDisposition::Never),
        may_have_submitted: classification.may_have_submitted,
        transition,
    }
}

fn retry_transition(
    state: TranscriptObservationPolicyState,
    classification: crate::TranscriptFailureClassification,
    retry_after_milliseconds: Option<u64>,
    observed_at: UnixTimestampMilliseconds,
    retry_issued_revision: StateRevision,
) -> TranscriptFailureTransition {
    if classification.retry == TranscriptRetryDisposition::AutomaticRequest
        && classification.resubmission_is_safe
        && state.attempt < state.max_attempts
    {
        let attempt = state.attempt.saturating_add(1).max(1);
        if let Some(attempt_id) = transcript_attempt_id(state.workflow_id, attempt) {
            let not_before = transcript_retry_not_before(
                observed_at,
                attempt,
                retry_after_milliseconds.and_then(|value| i64::try_from(value).ok()),
            );
            return TranscriptFailureTransition::Retry {
                attempt,
                attempt_id,
                submission_fence_id: transcript_submission_fence_id(attempt_id),
                request_id: transcript_workflow_request_id(state.workflow_id, attempt, false),
                issued_revision: retry_issued_revision,
                not_before,
                deadline_at: UnixTimestampMilliseconds::new(
                    not_before
                        .value
                        .saturating_add(TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS),
                ),
                evidence_permits_resubmission: true,
            };
        }
    }
    match classification.retry {
        TranscriptRetryDisposition::RecoverPersisted => {
            TranscriptFailureTransition::RecoverPersisted
        }
        TranscriptRetryDisposition::Replan => TranscriptFailureTransition::Block,
        TranscriptRetryDisposition::ExplicitOnly if classification.may_have_submitted => {
            TranscriptFailureTransition::Ambiguous
        }
        _ if classification.code == TranscriptWorkflowFailureCode::Cancelled => {
            TranscriptFailureTransition::Cancel
        }
        _ => TranscriptFailureTransition::Fail,
    }
}

fn bounded_provider_delay(value: Option<u64>) -> i64 {
    value
        .and_then(|milliseconds| i64::try_from(milliseconds).ok())
        .unwrap_or(TRANSCRIPT_RETRY_BASE_MILLISECONDS)
        .max(TRANSCRIPT_RETRY_BASE_MILLISECONDS)
}
