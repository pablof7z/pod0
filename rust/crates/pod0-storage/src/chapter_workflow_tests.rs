use pod0_domain::{CommandId, StateRevision};

use crate::chapter_workflow_test_support::*;
use crate::{
    LibraryStore, PublisherChapterWorkflowState, PublisherChapterWorkflowUpdate, StorageError,
};

#[test]
fn request_identity_and_retry_generation_survive_reopen() {
    let (fixture, store, episode_id) = workflow_fixture();
    let first = ensure(&store, episode_id, 1, false);
    let first_request = first.request_id.unwrap();
    drop(store);

    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(
        reopened
            .publisher_chapter_workflow(episode_id)
            .unwrap()
            .unwrap()
            .request_id,
        Some(first_request)
    );
    let existing = ensure(&reopened, episode_id, 2, false);
    assert_eq!(existing.request_id, Some(first_request));

    let retry = reopened
        .fail_publisher_chapter_workflow(failure(
            first_request,
            "offline",
            Some(2_000),
            StateRevision::new(3),
            Some(32_000),
            1_100,
        ))
        .unwrap();
    let PublisherChapterWorkflowUpdate::RetryScheduled(retry) = retry else {
        panic!("first transient failure must schedule a retry")
    };
    assert_eq!(retry.state, PublisherChapterWorkflowState::RetryScheduled);
    assert_eq!(retry.attempt, 2);
    assert_eq!(retry.generation, 2);
    assert_ne!(retry.request_id, Some(first_request));
    assert_eq!(retry.not_before_ms, Some(2_000));
    assert_eq!(
        LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .publisher_chapter_workflow(episode_id)
            .unwrap()
            .unwrap(),
        retry
    );
}

#[test]
fn completion_commits_and_selects_the_artifact_atomically() {
    let (_fixture, store, episode_id) = workflow_fixture();
    let requested = ensure(&store, episode_id, 1, false);
    let artifact = publisher_artifact(episode_id);
    let completed = store
        .complete_publisher_chapter_workflow(requested.request_id.unwrap(), artifact, 1_200)
        .unwrap();

    assert_eq!(completed.state, PublisherChapterWorkflowState::Succeeded);
    assert_eq!(
        store
            .selected_chapter_artifact(episode_id)
            .unwrap()
            .unwrap()
            .artifact
            .artifact_id,
        completed.selected_artifact_id.unwrap()
    );
}

#[test]
fn current_selected_legacy_publisher_artifact_is_adopted_without_a_fetch() {
    let (_fixture, store, episode_id) = workflow_fixture();
    store
        .commit_and_select_chapter(
            CommandId::from_parts(8, 1),
            StateRevision::INITIAL,
            current_publisher_artifact(episode_id),
            1_100,
        )
        .unwrap();

    let adopted = ensure(&store, episode_id, 2, false);

    assert_eq!(adopted.state, PublisherChapterWorkflowState::Succeeded);
    assert_eq!(adopted.generation, 1);
    assert!(adopted.request_id.is_none());
    assert_eq!(
        adopted.selected_artifact_id,
        store
            .selected_chapter_artifact(episode_id)
            .unwrap()
            .map(|selected| selected.artifact.artifact_id)
    );
}

#[test]
fn source_removal_preserves_generation_and_readd_uses_a_new_request_identity() {
    let (fixture, store, episode_id) = workflow_fixture();
    let first = ensure(&store, episode_id, 1, false);
    set_optional_chapter_source(&fixture, episode_id, None);
    let removed = store
        .mark_publisher_chapter_source_absent(episode_id, 1_200)
        .unwrap()
        .unwrap();
    let tombstone = store
        .publisher_chapter_workflow(episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(removed.request_id, first.request_id);
    assert_eq!(tombstone.state, PublisherChapterWorkflowState::SourceAbsent);
    assert_eq!(tombstone.generation, first.generation);
    assert!(tombstone.request_id.is_none());

    set_chapter_source(&fixture, episode_id, SOURCE_URL);
    let readded = ensure(&store, episode_id, 2, false);
    assert_eq!(readded.generation, first.generation + 1);
    assert_ne!(readded.request_id, first.request_id);
}

#[test]
fn later_chapter_selection_does_not_reopen_current_publisher_acquisition() {
    let (_fixture, store, episode_id) = workflow_fixture();
    let requested = ensure(&store, episode_id, 1, false);
    let completed = store
        .complete_publisher_chapter_workflow(
            requested.request_id.unwrap(),
            publisher_artifact(episode_id),
            1_200,
        )
        .unwrap();
    let selected = store
        .selected_chapter_artifact(episode_id)
        .unwrap()
        .unwrap();
    let mut replacement = publisher_artifact(episode_id);
    replacement.chapters[0].title = "Enriched opening".to_owned();
    store
        .commit_and_select_chapter(
            CommandId::from_parts(8, 8),
            selected.selection_revision,
            replacement,
            1_300,
        )
        .unwrap();

    let ensured = ensure(&store, episode_id, 2, false);
    assert_eq!(ensured, completed);
    assert_eq!(ensured.state, PublisherChapterWorkflowState::Succeeded);
}

#[test]
fn changed_source_fences_the_stale_response() {
    let (fixture, store, episode_id) = workflow_fixture();
    let requested = ensure(&store, episode_id, 1, false);
    set_chapter_source(
        &fixture,
        episode_id,
        "https://example.test/replacement.json",
    );

    assert_eq!(
        store
            .complete_publisher_chapter_workflow(
                requested.request_id.unwrap(),
                publisher_artifact(episode_id),
                1_300,
            )
            .unwrap_err(),
        StorageError::ChapterWorkflowConflict
    );
    assert!(
        store
            .selected_chapter_artifact(episode_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn retry_exhaustion_and_revision_fenced_cancellation_are_durable() {
    let (_fixture, store, episode_id) = workflow_fixture();
    let mut record = ensure(&store, episode_id, 1, false);
    for attempt in 1_u16..5 {
        let update = store
            .fail_publisher_chapter_workflow(failure(
                record.request_id.unwrap(),
                "transport",
                Some(2_000 + i64::from(attempt)),
                StateRevision::new(2 + u64::from(attempt)),
                Some(32_000 + i64::from(attempt)),
                1_100 + i64::from(attempt),
            ))
            .unwrap();
        let PublisherChapterWorkflowUpdate::RetryScheduled(next) = update else {
            panic!("attempt {attempt} must remain retryable")
        };
        record = next;
    }
    let exhausted = store
        .fail_publisher_chapter_workflow(failure(
            record.request_id.unwrap(),
            "transport",
            Some(9_000),
            StateRevision::new(9),
            Some(39_000),
            1_900,
        ))
        .unwrap();
    let PublisherChapterWorkflowUpdate::Failed(exhausted) = exhausted else {
        panic!("the fifth failed attempt must exhaust automatic retry")
    };
    assert_eq!(exhausted.attempt, 5);
    assert_eq!(exhausted.state, PublisherChapterWorkflowState::Failed);

    let retried = ensure(&store, episode_id, 10, true);
    assert_eq!(retried.attempt, 1);
    assert_eq!(
        store
            .cancel_publisher_chapter_workflow(
                episode_id,
                StateRevision::new(retried.workflow_revision.value - 1),
                2_000,
            )
            .unwrap_err(),
        StorageError::ChapterWorkflowConflict
    );
    assert_eq!(
        store
            .cancel_publisher_chapter_workflow(episode_id, retried.workflow_revision, 2_001,)
            .unwrap()
            .state,
        PublisherChapterWorkflowState::Cancelled
    );
}
