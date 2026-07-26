use super::tests::Fixture;
use super::*;

#[test]
fn post_claim_replan_requires_durable_completion_evidence() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(10, None);
    let ModelChapterSubmissionClaim::Authorized(authorized) =
        fixture.claim(&requested, 1_800_000_100_030)
    else {
        panic!("claim must authorize")
    };
    assert_eq!(
        fixture
            .store
            .fail_model_chapter_workflow(ModelChapterFailureInput {
                episode_id: authorized.episode_id,
                request_id: authorized.request_id.unwrap(),
                generation: authorized.generation,
                submission_fence_id: authorized.submission_fence_id.unwrap(),
                failure_code: "stale_transcript".to_owned(),
                failure_detail: None,
                may_have_submitted: true,
                disposition: ModelChapterFailureDisposition::Replan,
                observed_at_ms: 1_800_000_100_031,
            }),
        Err(crate::StorageError::ChapterWorkflowConflict)
    );
    assert_eq!(
        fixture
            .store
            .model_chapter_workflow(fixture.episode_id)
            .unwrap()
            .unwrap()
            .state,
        ModelChapterWorkflowState::SubmissionAuthorized
    );
}
