use super::model::*;
use super::test_support::{Fixture, NOW, interrupted};
use crate::StorageError;
use crate::transcript_store_test_support::input as artifact_input;

#[test]
fn submission_provider_completion_and_evidence_transactions_survive_interruption() {
    let fixture = Fixture::new();
    let requested = fixture.ensure_provider(1);
    let request_id = requested.request_id.unwrap();
    let attempt_id = requested.attempt_id.unwrap();
    let fence = requested.submission_fence_id.unwrap();
    let claim = TranscriptSubmissionClaimInput {
        episode_id: fixture.episode_id,
        request_id,
        attempt_id,
        submission_fence_id: fence,
        cancellation_id: requested.cancellation_id,
        issued_revision: requested.issued_revision,
        now_ms: NOW + 2,
    };

    assert_eq!(
        fixture
            .store
            .claim_transcript_submission_with_observer(claim, interrupted),
        Err(StorageError::Interrupted)
    );
    assert_stage(&fixture, StoredTranscriptWorkflowStage::Requested);
    fixture.store.claim_transcript_submission(claim).unwrap();

    let accepted = TranscriptProviderAcceptedInput {
        episode_id: fixture.episode_id,
        request_id,
        attempt_id,
        submission_fence_id: fence,
        external_operation_id: "provider-operation-1".to_owned(),
        provider_status: Some("processing".to_owned()),
        observed_at_ms: NOW + 3,
    };
    assert_eq!(
        fixture
            .store
            .record_transcript_provider_accepted_with_observer(accepted.clone(), interrupted),
        Err(StorageError::Interrupted)
    );
    assert_stage(
        &fixture,
        StoredTranscriptWorkflowStage::SubmissionAuthorized,
    );
    fixture
        .store
        .record_transcript_provider_accepted(accepted)
        .unwrap();

    let completion = TranscriptCompletionInput {
        episode_id: fixture.episode_id,
        request_id,
        attempt_id: Some(attempt_id),
        submission_fence_id: Some(fence),
        external_operation_id: Some("provider-operation-1".to_owned()),
        provider_status: Some("completed".to_owned()),
        artifact: artifact_input("audio-v1"),
        observed_at_ms: NOW + 4,
    };
    assert_eq!(
        fixture
            .store
            .stage_transcript_workflow_completion_with_observer(completion.clone(), interrupted,),
        Err(StorageError::Interrupted)
    );
    assert_stage(&fixture, StoredTranscriptWorkflowStage::ProviderAccepted);
    fixture
        .store
        .stage_transcript_workflow_completion(completion)
        .unwrap();

    let committed = fixture
        .store
        .commit_transcript_workflow(TranscriptWorkflowCommitInput {
            episode_id: fixture.episode_id,
            request_id,
            evidence_input_version: "evidence-v1".to_owned(),
            completed_at_ms: NOW + 5,
        })
        .unwrap();
    assert_eq!(
        fixture
            .store
            .complete_transcript_evidence_request_with_observer(
                committed.workflow.request.workflow_id,
                "evidence-v1",
                NOW + 6,
                interrupted,
            ),
        Err(StorageError::Interrupted)
    );
    assert_stage(&fixture, StoredTranscriptWorkflowStage::EvidenceRequested);
    fixture
        .store
        .complete_transcript_evidence_request(
            committed.workflow.request.workflow_id,
            "evidence-v1",
            NOW + 6,
        )
        .unwrap();
    assert_stage(&fixture, StoredTranscriptWorkflowStage::Succeeded);
}

fn assert_stage(fixture: &Fixture, expected: StoredTranscriptWorkflowStage) {
    assert_eq!(
        fixture
            .reopen()
            .transcript_workflow(fixture.episode_id)
            .unwrap()
            .unwrap()
            .stage,
        expected
    );
}
