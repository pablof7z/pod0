use pod0_domain::StateRevision;

use super::model::*;
use super::test_support::{Fixture, NOW, interrupted};
use crate::StorageError;
use crate::transcript_store_test_support::input as artifact_input;

#[test]
fn submission_authorization_is_durable_and_idempotently_fenced() {
    let fixture = Fixture::new();
    let requested = fixture.ensure_provider(1);
    let claim_input = TranscriptSubmissionClaimInput {
        episode_id: fixture.episode_id,
        request_id: requested.request_id.unwrap(),
        attempt_id: requested.attempt_id.unwrap(),
        submission_fence_id: requested.submission_fence_id.unwrap(),
        cancellation_id: requested.cancellation_id,
        issued_revision: requested.issued_revision,
        now_ms: NOW + 2,
    };

    let TranscriptSubmissionClaim::Authorized(authorized) = fixture
        .store
        .claim_transcript_submission(claim_input)
        .unwrap()
    else {
        panic!("first claim must authorize")
    };
    assert_eq!(
        authorized.stage,
        StoredTranscriptWorkflowStage::SubmissionAuthorized
    );
    assert!(authorized.may_have_submitted);

    let reopened = fixture.reopen();
    let TranscriptSubmissionClaim::AlreadyClaimed(replayed) =
        reopened.claim_transcript_submission(claim_input).unwrap()
    else {
        panic!("replayed claim must not authorize again")
    };
    assert_eq!(replayed.workflow_revision, authorized.workflow_revision);

    let stale = TranscriptProviderAcceptedInput {
        episode_id: fixture.episode_id,
        request_id: requested.request_id.unwrap(),
        attempt_id: pod0_domain::TranscriptAttemptId::from_bytes([0xEE; 16]),
        submission_fence_id: requested.submission_fence_id.unwrap(),
        external_operation_id: "provider-1".to_owned(),
        provider_status: None,
        observed_at_ms: NOW + 3,
    };
    assert_eq!(
        reopened.record_transcript_provider_accepted(stale),
        Err(StorageError::StaleTranscriptAttempt)
    );
}

#[test]
fn accepted_provider_and_completion_recover_without_resubmission() {
    let fixture = Fixture::new();
    let requested = fixture.ensure_provider(1);
    let attempt_id = requested.attempt_id.unwrap();
    let fence = requested.submission_fence_id.unwrap();
    let request_id = requested.request_id.unwrap();
    fixture
        .store
        .claim_transcript_submission(TranscriptSubmissionClaimInput {
            episode_id: fixture.episode_id,
            request_id,
            attempt_id,
            submission_fence_id: fence,
            cancellation_id: requested.cancellation_id,
            issued_revision: requested.issued_revision,
            now_ms: NOW + 2,
        })
        .unwrap();
    let accepted = fixture
        .store
        .record_transcript_provider_accepted(TranscriptProviderAcceptedInput {
            episode_id: fixture.episode_id,
            request_id,
            attempt_id,
            submission_fence_id: fence,
            external_operation_id: "provider-operation-1".to_owned(),
            provider_status: Some("processing".to_owned()),
            observed_at_ms: NOW + 3,
        })
        .unwrap();
    assert_eq!(
        accepted.stage,
        StoredTranscriptWorkflowStage::ProviderAccepted
    );

    let recovered = fixture
        .reopen()
        .recover_transcript_workflows(NOW + 4, 20)
        .unwrap();
    assert_eq!(recovered.provider_recoveries, [request_id]);
    assert!(recovered.dispatchable_requests.is_empty());

    let completed = fixture
        .store
        .stage_transcript_workflow_completion(TranscriptCompletionInput {
            episode_id: fixture.episode_id,
            request_id,
            attempt_id: Some(attempt_id),
            submission_fence_id: Some(fence),
            external_operation_id: Some("provider-operation-1".to_owned()),
            provider_status: Some("completed".to_owned()),
            artifact: artifact_input("audio-v1"),
            observed_at_ms: NOW + 5,
        })
        .unwrap();
    assert_eq!(
        completed.stage,
        StoredTranscriptWorkflowStage::CompletionObserved
    );
    let report = fixture
        .reopen()
        .recover_transcript_workflows(NOW + 6, 20)
        .unwrap();
    assert_eq!(report.completions_to_commit, [request_id]);
}

#[test]
fn transcript_selection_and_evidence_admission_commit_atomically() {
    let fixture = Fixture::new();
    let requested = fixture.ensure_provider(1);
    let request_id = requested.request_id.unwrap();
    let attempt_id = requested.attempt_id.unwrap();
    let fence = requested.submission_fence_id.unwrap();
    fixture
        .store
        .claim_transcript_submission(TranscriptSubmissionClaimInput {
            episode_id: fixture.episode_id,
            request_id,
            attempt_id,
            submission_fence_id: fence,
            cancellation_id: requested.cancellation_id,
            issued_revision: requested.issued_revision,
            now_ms: NOW + 2,
        })
        .unwrap();
    fixture
        .store
        .stage_transcript_workflow_completion(TranscriptCompletionInput {
            episode_id: fixture.episode_id,
            request_id,
            attempt_id: Some(attempt_id),
            submission_fence_id: Some(fence),
            external_operation_id: None,
            provider_status: None,
            artifact: artifact_input("audio-v1"),
            observed_at_ms: NOW + 3,
        })
        .unwrap();
    let commit = TranscriptWorkflowCommitInput {
        episode_id: fixture.episode_id,
        request_id,
        evidence_input_version: "evidence-v1".to_owned(),
        completed_at_ms: NOW + 4,
    };

    assert_eq!(
        fixture
            .store
            .commit_transcript_workflow_with_observer(commit.clone(), interrupted),
        Err(StorageError::Interrupted)
    );
    assert!(
        fixture
            .transcript
            .store
            .selected_summary(fixture.episode_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .store
            .transcript_workflow(fixture.episode_id)
            .unwrap()
            .unwrap()
            .stage,
        StoredTranscriptWorkflowStage::CompletionObserved
    );

    let committed = fixture
        .store
        .commit_transcript_workflow(commit.clone())
        .unwrap();
    assert_eq!(
        committed.workflow.stage,
        StoredTranscriptWorkflowStage::EvidenceRequested
    );
    assert_eq!(
        committed.transcript.selection_revision,
        StateRevision::new(1)
    );
    let replay = fixture.reopen().commit_transcript_workflow(commit).unwrap();
    assert_eq!(replay.transcript, committed.transcript);
    let report = fixture
        .store
        .recover_transcript_workflows(NOW + 5, 20)
        .unwrap();
    assert_eq!(
        report.evidence_requests,
        [committed.workflow.request.workflow_id]
    );

    let succeeded = fixture
        .store
        .complete_transcript_evidence_request(
            committed.workflow.request.workflow_id,
            "evidence-v1",
            NOW + 6,
        )
        .unwrap();
    assert_eq!(succeeded.stage, StoredTranscriptWorkflowStage::Succeeded);
}

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
}
