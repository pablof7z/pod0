use pod0_domain::{StateRevision, TranscriptWorkflowId, UnixTimestampMilliseconds};

use crate::{
    TranscriptCapabilityObservation, TranscriptFailureEvidence, TranscriptFailureTransition,
    TranscriptObservationDecision, TranscriptObservationPolicyInput,
    TranscriptObservationPolicyState, decide_transcript_observation,
};

#[test]
fn safe_pre_submission_failure_plans_a_new_fenced_attempt() {
    let decision = decide_transcript_observation(TranscriptObservationPolicyInput {
        state: TranscriptObservationPolicyState {
            workflow_id: TranscriptWorkflowId::from_parts(1, 2),
            workflow_revision: StateRevision::new(3),
            attempt: 1,
            max_attempts: 3,
            submission_authorized: false,
            provider_accepted: false,
        },
        observation: TranscriptCapabilityObservation::Failed {
            evidence: TranscriptFailureEvidence::Offline {
                submission_authorized: false,
                provider_accepted: false,
            },
            safe_detail: None,
            retry_after_milliseconds: None,
        },
        observed_at: UnixTimestampMilliseconds::new(1_000),
        retry_issued_revision: StateRevision::new(4),
    });

    assert!(matches!(
        decision,
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Retry {
                attempt: 2,
                issued_revision,
                ..
            },
            may_have_submitted: false,
            ..
        } if issued_revision == StateRevision::new(4)
    ));
}

#[test]
fn cancellation_after_submission_is_an_ambiguous_terminal_decision() {
    let decision = decide_transcript_observation(TranscriptObservationPolicyInput {
        state: TranscriptObservationPolicyState {
            workflow_id: TranscriptWorkflowId::from_parts(1, 2),
            workflow_revision: StateRevision::new(3),
            attempt: 1,
            max_attempts: 3,
            submission_authorized: true,
            provider_accepted: false,
        },
        observation: TranscriptCapabilityObservation::Cancelled,
        observed_at: UnixTimestampMilliseconds::new(1_000),
        retry_issued_revision: StateRevision::new(4),
    });

    assert!(matches!(
        decision,
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Cancel,
            may_have_submitted: true,
            ..
        }
    ));
}
