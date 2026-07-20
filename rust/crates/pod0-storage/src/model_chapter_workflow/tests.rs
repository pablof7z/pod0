use pod0_domain::{
    CancellationId, ChapterArtifactSource, CommandId, ContentDigest, EpisodeId, StateRevision,
    TranscriptArtifact,
};

use super::*;
use crate::transcript_store_test_support::{
    TranscriptFixture, command as transcript_command, input as transcript_input,
};
use crate::{LibraryStore, StorageError};

const NOW: i64 = 1_800_000_100_000;

pub(super) struct Fixture {
    pub(super) _transcript: TranscriptFixture,
    pub(super) store: LibraryStore,
    pub(super) episode_id: EpisodeId,
    pub(super) transcript_version_id: pod0_domain::TranscriptVersionId,
    pub(super) transcript_digest: ContentDigest,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let transcript = TranscriptFixture::new();
        let input = transcript_input("model-workflow-transcript-v1");
        let sealed = TranscriptArtifact::seal(input.clone()).unwrap();
        transcript
            .store
            .commit_and_select(
                transcript_command(9_100),
                StateRevision::INITIAL,
                input,
                NOW - 10,
            )
            .unwrap();
        let store = LibraryStore::open_authoritative(&transcript.import.target).unwrap();
        Self {
            _transcript: transcript,
            store,
            episode_id: sealed.episode_id,
            transcript_version_id: sealed.transcript_version_id,
            transcript_digest: sealed.content_digest,
        }
    }

    pub(super) fn request(&self, fingerprint: u8) -> StoredModelChapterRequest {
        StoredModelChapterRequest {
            configured_model: "openrouter:model-a".to_owned(),
            mode: ModelChapterWorkflowMode::Generate,
            source_version: format!("source-{fingerprint}"),
            request_fingerprint: ContentDigest::from_bytes([fingerprint; 32]),
            requested_transcript_version_id: self.transcript_version_id,
            requested_transcript_digest: self.transcript_digest,
            selected_transcript_version_id: self.transcript_version_id,
            selected_transcript_digest: self.transcript_digest,
            expected_selection_revision: StateRevision::INITIAL,
            base_artifact_id: None,
            base_integrity_digest: None,
            format_version: 1,
            policy_version: 1,
            provider: "openrouter".to_owned(),
            model: "model-a".to_owned(),
            response_format_code: 0,
            maximum_completion_bytes: 64 * 1_024,
            duration_ms: Some(120_000),
            expected_artifact_source: ChapterArtifactSource::Generated,
            system_prompt: "Return chapters as JSON.".to_owned(),
            user_prompt: "Transcript body".to_owned(),
        }
    }

    pub(super) fn ensure(
        &self,
        fingerprint: u8,
        force: Option<StateRevision>,
    ) -> ModelChapterWorkflowRecord {
        let input = ModelChapterEnsureInput {
            episode_id: self.episode_id,
            configured_model: "openrouter:model-a".to_owned(),
            desired_plan: ModelChapterDesiredPlan::Ready(Box::new(self.request(fingerprint))),
            command_id: CommandId::from_parts(20, u64::from(fingerprint)),
            cancellation_id: CancellationId::from_parts(21, u64::from(fingerprint)),
            issued_revision: StateRevision::new(u64::from(fingerprint)),
            now_ms: NOW + i64::from(fingerprint),
            request_deadline_ms: NOW + 60_000 + i64::from(fingerprint),
            max_attempts: 4,
            force_retry_from_revision: force,
        };
        match self.store.ensure_model_chapter_workflow(input).unwrap() {
            ModelChapterEnsureOutcome::Changed { record, .. }
            | ModelChapterEnsureOutcome::Existing(record) => record,
        }
    }

    pub(super) fn claim(
        &self,
        record: &ModelChapterWorkflowRecord,
        now_ms: i64,
    ) -> ModelChapterSubmissionClaim {
        self.store
            .claim_model_chapter_submission(ModelChapterSubmissionClaimInput {
                episode_id: record.episode_id,
                request_id: record.request_id.unwrap(),
                generation: record.generation,
                cancellation_id: record.cancellation_id,
                issued_revision: record.issued_revision,
                now_ms,
            })
            .unwrap()
    }

    pub(super) fn completion(
        &self,
        record: &ModelChapterWorkflowRecord,
    ) -> ModelChapterCompletionInput {
        ModelChapterCompletionInput {
            episode_id: record.episode_id,
            request_id: record.request_id.unwrap(),
            generation: record.generation,
            submission_fence_id: record.submission_fence_id.unwrap(),
            completion: r#"{"chapters":[{"start":0,"title":"Opening"},{"start":30,"title":"First"},{"start":60,"title":"Second"},{"start":90,"title":"Close"}]}"#.to_owned(),
            provider: "openrouter".to_owned(),
            model: "model-a".to_owned(),
            prompt_tokens: Some(100),
            completion_tokens: Some(20),
            cached_tokens: None,
            reasoning_tokens: None,
            cost_microusd: Some(12),
            provider_operation_id: Some("provider-job-1".to_owned()),
            provider_status: Some("completed".to_owned()),
            generated_at_ms: NOW + 100,
            observed_at_ms: NOW + 101,
        }
    }
}

#[test]
fn claim_precedes_post_and_raw_completion_is_durable_and_idempotent() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(1, None);
    assert_eq!(requested.state, ModelChapterWorkflowState::Requested);
    assert_eq!(fixture.ensure(1, None), requested);

    let ModelChapterSubmissionClaim::Authorized(authorized) = fixture.claim(&requested, NOW + 2)
    else {
        panic!("first claim must authorize exactly one POST")
    };
    assert_eq!(
        authorized.state,
        ModelChapterWorkflowState::SubmissionAuthorized
    );
    assert!(matches!(
        fixture.claim(&authorized, NOW + 3),
        ModelChapterSubmissionClaim::AlreadyClaimed(_)
    ));

    let completion_input = fixture.completion(&authorized);
    let completion = fixture
        .store
        .stage_model_chapter_completion(completion_input.clone())
        .unwrap();
    assert_eq!(
        fixture
            .store
            .model_chapter_workflow(fixture.episode_id)
            .unwrap()
            .unwrap()
            .state,
        ModelChapterWorkflowState::CompletionObserved
    );
    assert_eq!(
        fixture
            .store
            .stage_model_chapter_completion(completion_input)
            .unwrap(),
        completion
    );
    assert_eq!(
        fixture
            .store
            .model_chapter_completion(completion.request_id)
            .unwrap(),
        Some(completion)
    );
}

#[test]
fn restart_after_claim_becomes_ambiguous_and_never_reposts_implicitly() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(2, None);
    let ModelChapterSubmissionClaim::Authorized(authorized) = fixture.claim(&requested, NOW + 3)
    else {
        panic!("claim must authorize")
    };
    let report = fixture
        .store
        .recover_model_chapter_workflows(16, NOW + 4)
        .unwrap();
    assert_eq!(report.ambiguous_requests, [authorized.request_id.unwrap()]);
    let ambiguous = fixture
        .store
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(ambiguous.state, ModelChapterWorkflowState::Ambiguous);
    assert!(matches!(
        fixture.claim(&ambiguous, NOW + 5),
        ModelChapterSubmissionClaim::AlreadyClaimed(_)
    ));

    let retried = fixture.ensure(2, Some(ambiguous.workflow_revision));
    assert_eq!(retried.generation, 2);
    assert_ne!(retried.request_id, ambiguous.request_id);
    assert_eq!(retried.state, ModelChapterWorkflowState::Requested);
}

#[test]
fn stale_callbacks_and_unsafe_post_claim_retries_fail_closed() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(3, None);
    let ModelChapterSubmissionClaim::Authorized(authorized) = fixture.claim(&requested, NOW + 4)
    else {
        panic!("claim must authorize")
    };
    let mut stale = fixture.completion(&authorized);
    stale.generation += 1;
    assert_eq!(
        fixture.store.stage_model_chapter_completion(stale),
        Err(StorageError::ChapterWorkflowConflict)
    );
    let failure = ModelChapterFailureInput {
        episode_id: authorized.episode_id,
        request_id: authorized.request_id.unwrap(),
        generation: authorized.generation,
        submission_fence_id: authorized.submission_fence_id.unwrap(),
        failure_code: "transport".to_owned(),
        failure_detail: None,
        may_have_submitted: true,
        disposition: ModelChapterFailureDisposition::Retry {
            not_before_ms: NOW + 10,
            deadline_at_ms: NOW + 70_000,
            issued_revision: StateRevision::new(99),
            evidence_permits_resubmission: false,
        },
        observed_at_ms: NOW + 5,
    };
    assert_eq!(
        fixture.store.fail_model_chapter_workflow(failure),
        Err(StorageError::ChapterWorkflowConflict)
    );
}

#[test]
fn changed_inputs_after_claim_are_deferred_without_overwriting_active_evidence() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(4, None);
    let ModelChapterSubmissionClaim::Authorized(active) = fixture.claim(&requested, NOW + 5) else {
        panic!("claim must authorize")
    };
    let changed = fixture.ensure(5, None);
    assert_eq!(changed.request_id, active.request_id);
    assert_eq!(changed.generation, active.generation);
    assert_eq!(changed.active_request, active.active_request);
    assert!(changed.replan_pending);
}

#[test]
fn invalid_transcript_provenance_is_rejected_before_request_persistence() {
    let fixture = Fixture::new();
    let mut input = ModelChapterEnsureInput {
        episode_id: fixture.episode_id,
        configured_model: "openrouter:model-a".to_owned(),
        desired_plan: ModelChapterDesiredPlan::Ready(Box::new(fixture.request(6))),
        command_id: CommandId::from_parts(30, 1),
        cancellation_id: CancellationId::from_parts(31, 1),
        issued_revision: StateRevision::new(1),
        now_ms: NOW,
        request_deadline_ms: NOW + 60_000,
        max_attempts: 4,
        force_retry_from_revision: None,
    };
    let ModelChapterDesiredPlan::Ready(request) = &mut input.desired_plan else {
        unreachable!()
    };
    request.selected_transcript_digest = ContentDigest::from_bytes([0xff; 32]);
    assert_eq!(
        fixture.store.ensure_model_chapter_workflow(input),
        Err(StorageError::ChapterWorkflowConflict)
    );
}
