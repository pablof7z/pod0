use pod0_domain::{StateRevision, TranscriptWorkflowId, UnixTimestampMilliseconds};

use crate::{
    ActivityFailureCode, EffectOutcome, MAX_TRANSCRIPT_SAFE_DETAIL_BYTES,
    TranscriptCapabilityObservation, TranscriptCapabilityValidation, TranscriptFailureEvidence,
    TranscriptFailureTransition, TranscriptObservationDecision, TranscriptObservationPolicyInput,
    TranscriptObservationPolicyState, TranscriptTransition, TranscriptWorkflowFailureCode,
    decide_transcript_observation, transcript_observation_semantics,
    validate_transcript_capability_observation,
};

fn decide(
    observation: TranscriptCapabilityObservation,
    submission_authorized: bool,
    provider_accepted: bool,
) -> TranscriptObservationDecision {
    decide_transcript_observation(TranscriptObservationPolicyInput {
        state: TranscriptObservationPolicyState {
            workflow_id: TranscriptWorkflowId::from_parts(1, 2),
            workflow_revision: StateRevision::new(3),
            attempt: 1,
            max_attempts: 3,
            submission_authorized,
            provider_accepted,
        },
        observation,
        observed_at: UnixTimestampMilliseconds::new(1_000),
        retry_issued_revision: StateRevision::new(4),
    })
}

fn failed(evidence: TranscriptFailureEvidence) -> TranscriptCapabilityObservation {
    TranscriptCapabilityObservation::Failed {
        evidence,
        safe_detail: None,
        retry_after_milliseconds: None,
    }
}

#[test]
fn rust_maps_progress_success_rejection_retry_cancellation_and_unknown() {
    let accepted = decide(
        TranscriptCapabilityObservation::ProviderAccepted {
            external_operation_id: "operation-1".to_owned(),
            provider_status: Some("queued".to_owned()),
        },
        true,
        false,
    );
    assert_eq!(
        transcript_observation_semantics(&accepted),
        (
            EffectOutcome::Progressed,
            TranscriptTransition::AttemptStateChanged,
        )
    );

    let completed = TranscriptObservationDecision::Completion;
    assert_eq!(
        transcript_observation_semantics(&completed),
        (
            EffectOutcome::Succeeded,
            TranscriptTransition::ArtifactAdopted,
        )
    );

    let rejected = decide(
        failed(TranscriptFailureEvidence::ProviderRejected),
        true,
        true,
    );
    assert!(matches!(
        rejected,
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Fail,
            ..
        }
    ));
    assert_eq!(
        transcript_observation_semantics(&rejected),
        (
            EffectOutcome::Failed {
                code: ActivityFailureCode::InvalidResponse,
            },
            TranscriptTransition::AttemptStateChanged,
        )
    );

    let retryable = decide(failed(TranscriptFailureEvidence::Offline), false, false);
    assert!(matches!(
        retryable,
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Retry { .. },
            ..
        }
    ));
    assert_eq!(
        transcript_observation_semantics(&retryable).0,
        EffectOutcome::Failed {
            code: ActivityFailureCode::Offline,
        }
    );

    let cancelled = decide(TranscriptCapabilityObservation::Cancelled, true, false);
    assert_eq!(
        transcript_observation_semantics(&cancelled),
        (EffectOutcome::Cancelled, TranscriptTransition::Cancelled)
    );

    let unknown = decide(failed(TranscriptFailureEvidence::Transport), true, false);
    assert!(matches!(
        unknown,
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Ambiguous,
            ..
        }
    ));
    assert_eq!(
        transcript_observation_semantics(&unknown),
        (
            EffectOutcome::OutcomeUnknown,
            TranscriptTransition::AttemptStateChanged,
        )
    );
}

#[test]
fn identical_raw_transport_evidence_uses_only_rust_owned_phase() {
    let raw = failed(TranscriptFailureEvidence::Transport);
    let encoded = serde_json::to_string(&raw).unwrap();
    assert!(!encoded.contains("submission_authorized"));
    assert!(!encoded.contains("provider_accepted"));
    assert!(!encoded.contains("may_have_submitted"));

    let before = decide(raw.clone(), false, false);
    let authorized = decide(raw.clone(), true, false);
    let accepted = decide(raw, true, true);

    assert!(matches!(
        before,
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Retry { .. },
            may_have_submitted: false,
            ..
        }
    ));
    assert!(matches!(
        authorized,
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Ambiguous,
            may_have_submitted: true,
            ..
        }
    ));
    assert!(matches!(
        accepted,
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::RecoverPersisted,
            may_have_submitted: true,
            ..
        }
    ));
}

#[test]
fn raw_failure_detail_is_bounded_before_policy_mapping() {
    let observation = TranscriptCapabilityObservation::Failed {
        evidence: TranscriptFailureEvidence::InvalidResponse,
        safe_detail: Some("x".repeat(MAX_TRANSCRIPT_SAFE_DETAIL_BYTES + 1)),
        retry_after_milliseconds: None,
    };
    assert_eq!(
        validate_transcript_capability_observation(observation),
        TranscriptCapabilityValidation::Rejected {
            code: TranscriptWorkflowFailureCode::InvalidResponse,
        }
    );
}
