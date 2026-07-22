use pod0_domain::StateRevision;

use super::model::*;
use super::test_support::{Fixture, NOW};
use crate::TranscriptWorkflowAuthorityState;

#[test]
fn retry_requires_explicit_safe_evidence_and_creates_a_new_fence() {
    let fixture = Fixture::new();
    let requested = fixture.ensure_provider(1);
    let next = fixture.attempt(2);
    let retry = fixture
        .store
        .fail_transcript_workflow(TranscriptWorkflowFailureInput {
            episode_id: fixture.episode_id,
            request_id: requested.request_id.unwrap(),
            attempt_id: requested.attempt_id,
            submission_fence_id: requested.submission_fence_id,
            failure_code: "offline".to_owned(),
            failure_detail: None,
            retryable: true,
            may_have_submitted: false,
            disposition: TranscriptWorkflowFailureDisposition::Retry {
                attempt: next,
                request_id: pod0_domain::HostRequestId::from_parts(94, 2),
                issued_revision: StateRevision::new(2),
                not_before_ms: NOW + 10,
                deadline_at_ms: NOW + 60_000,
                evidence_permits_resubmission: true,
            },
            observed_at_ms: NOW + 3,
        })
        .unwrap();
    assert_eq!(retry.stage, StoredTranscriptWorkflowStage::RetryScheduled);
    assert_ne!(retry.submission_fence_id, requested.submission_fence_id);
    assert_eq!(
        fixture
            .store
            .recover_transcript_workflows(NOW + 9, 20)
            .unwrap()
            .dispatchable_requests,
        []
    );
    assert_eq!(
        fixture
            .store
            .recover_transcript_workflows(NOW + 10, 20)
            .unwrap()
            .dispatchable_requests,
        [retry.request_id.unwrap()]
    );
}

#[test]
fn authority_is_explicit_and_survives_restart() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.store.transcript_workflow_authority().unwrap(),
        TranscriptWorkflowAuthorityState::Authoritative {
            source_generation: 1
        }
    );
    assert_eq!(
        fixture.reopen().transcript_workflow_authority().unwrap(),
        TranscriptWorkflowAuthorityState::Authoritative {
            source_generation: 1
        }
    );
}
