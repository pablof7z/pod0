use pod0_domain::{CancellationId, CommandId, StateRevision};

use super::tests::Fixture;
use super::*;
use crate::StorageError;

const NOW: i64 = 1_800_000_100_050;

#[test]
fn provider_updates_form_one_ordered_operation_stream() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(11, None);
    let ModelChapterSubmissionClaim::Authorized(authorized) = fixture.claim(&requested, NOW) else {
        panic!("claim must authorize")
    };
    let input = |operation: &str, status: &str, observed_at_ms| ModelChapterProviderAcceptedInput {
        episode_id: authorized.episode_id,
        request_id: authorized.request_id.unwrap(),
        generation: authorized.generation,
        submission_fence_id: authorized.submission_fence_id.unwrap(),
        provider_operation_id: operation.into(),
        provider_status: Some(status.into()),
        observed_at_ms,
    };
    fixture
        .store
        .record_model_chapter_provider_accepted(input("provider-operation-1", "queued", NOW + 1))
        .unwrap();
    let running = fixture
        .store
        .record_model_chapter_provider_accepted(input("provider-operation-1", "running", NOW + 2))
        .unwrap();
    assert_eq!(running.provider_status.as_deref(), Some("running"));
    assert_eq!(
        fixture.store.record_model_chapter_provider_accepted(input(
            "provider-operation-1",
            "queued",
            NOW + 1,
        )),
        Err(StorageError::ChapterWorkflowConflict)
    );
    assert_eq!(
        fixture.store.record_model_chapter_provider_accepted(input(
            "provider-operation-2",
            "running",
            NOW + 3,
        )),
        Err(StorageError::ChapterWorkflowConflict)
    );
    let mut mismatched_completion = fixture.completion(&authorized);
    mismatched_completion.provider_operation_id = Some("provider-operation-2".into());
    assert_eq!(
        fixture
            .store
            .stage_model_chapter_completion(mismatched_completion),
        Err(StorageError::ChapterWorkflowConflict)
    );
}

#[test]
fn exact_late_completion_can_resolve_an_ambiguous_submission() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(12, None);
    let ModelChapterSubmissionClaim::Authorized(authorized) = fixture.claim(&requested, NOW) else {
        panic!("claim must authorize")
    };
    let ambiguous = fixture
        .store
        .fail_model_chapter_workflow(ModelChapterFailureInput {
            episode_id: authorized.episode_id,
            request_id: authorized.request_id.unwrap(),
            generation: authorized.generation,
            submission_fence_id: authorized.submission_fence_id.unwrap(),
            failure_code: "ambiguous_submission".into(),
            failure_detail: None,
            may_have_submitted: true,
            disposition: ModelChapterFailureDisposition::Ambiguous,
            observed_at_ms: NOW + 1,
        })
        .unwrap();
    assert_eq!(ambiguous.state, ModelChapterWorkflowState::Ambiguous);

    fixture
        .store
        .stage_model_chapter_completion(fixture.completion(&authorized))
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
}

#[test]
fn blocked_planner_detail_is_bounded_before_persistence() {
    let fixture = Fixture::new();
    let result = fixture
        .store
        .ensure_model_chapter_workflow(ModelChapterEnsureInput {
            episode_id: fixture.episode_id,
            configured_model: "ollama:model-a".into(),
            desired_plan: ModelChapterDesiredPlan::Blocked {
                failure_code: "invalid_request".into(),
                failure_detail: Some("x".repeat(16_385)),
            },
            command_id: CommandId::from_parts(30, 12),
            cancellation_id: CancellationId::from_parts(31, 12),
            issued_revision: StateRevision::new(12),
            now_ms: NOW,
            request_deadline_ms: NOW + 60_000,
            max_attempts: 4,
            force_retry_from_revision: None,
        });
    assert_eq!(result, Err(StorageError::ChapterWorkflowConflict));
}
