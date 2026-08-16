use super::model::{StoredTranscriptWorkflowStage, TranscriptSubmissionClaimInput};
use super::test_support::{Fixture, NOW};
use pod0_application::{ActivityActor, ActivityFact};

#[test]
fn restart_fences_authorized_but_unaccepted_submission_as_ambiguous() {
    let fixture = Fixture::new();
    let requested = fixture.ensure_provider(1);
    let request_id = requested.request_id.unwrap();
    fixture
        .store
        .claim_transcript_submission(TranscriptSubmissionClaimInput {
            episode_id: fixture.episode_id,
            request_id,
            attempt_id: requested.attempt_id.unwrap(),
            submission_fence_id: requested.submission_fence_id.unwrap(),
            cancellation_id: requested.cancellation_id,
            issued_revision: requested.issued_revision,
            now_ms: NOW + 2,
        })
        .unwrap();
    let report = fixture
        .reopen()
        .recover_transcript_workflows(NOW + 3, 20)
        .unwrap();
    assert_eq!(report.ambiguous_requests, [request_id]);
    assert!(report.dispatchable_requests.is_empty());
    let blocked = fixture
        .store
        .transcript_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(blocked.stage, StoredTranscriptWorkflowStage::Blocked);
    assert_eq!(
        blocked.failure_code.as_deref(),
        Some("ambiguous_submission")
    );
    let page = crate::ActivityStore::open(&fixture.transcript.import.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap();
    let recovery = page
        .items
        .iter()
        .filter(|item| item.draft.actor == ActivityActor::Recovery)
        .collect::<Vec<_>>();
    assert_eq!(recovery.len(), 2);
    assert!(matches!(
        recovery[1].draft.fact,
        ActivityFact::DomainTransition { .. }
    ));
    assert!(
        !serde_json::to_string(&recovery)
            .unwrap()
            .contains("provider recovery or explicit review")
    );
}
