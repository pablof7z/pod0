use pod0_domain::{CancellationId, CommandId};

use super::cutover_test_support::{NOW, cutover, entry};
use super::tests::Fixture;
use super::*;
use crate::StorageError;

fn discard(
    fixture: &Fixture,
    source_generation: u64,
) -> Result<ModelChapterWorkflowAuthorityState, StorageError> {
    fixture
        .store
        .discard_staged_legacy_model_chapter_workflow_cutover(
            source_generation,
            CommandId::from_parts(70, source_generation),
            CancellationId::from_parts(71, source_generation),
        )
}

#[test]
fn staged_discard_is_generation_fenced_and_allows_a_fresh_source() {
    let fixture = Fixture::new();
    fixture
        .store
        .stage_legacy_model_chapter_workflow_cutover(cutover(
            80,
            vec![entry(
                &fixture,
                fixture.request(80),
                LegacyModelChapterWorkflowDisposition::Ambiguous,
            )],
        ))
        .unwrap();

    assert_eq!(
        discard(&fixture, 81),
        Err(StorageError::ChapterWorkflowConflict)
    );
    assert_eq!(
        fixture.store.model_chapter_workflow_authority().unwrap(),
        ModelChapterWorkflowAuthorityState::Staged {
            source_generation: 80
        }
    );
    assert!(
        fixture
            .store
            .model_chapter_workflow(fixture.episode_id)
            .unwrap()
            .is_some()
    );

    assert_eq!(
        discard(&fixture, 80).unwrap(),
        ModelChapterWorkflowAuthorityState::NotStarted
    );
    assert!(
        fixture
            .store
            .model_chapter_workflow(fixture.episode_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .store
            .stage_legacy_model_chapter_workflow_cutover(cutover(
                81,
                vec![entry(
                    &fixture,
                    fixture.request(81),
                    LegacyModelChapterWorkflowDisposition::Ambiguous,
                )],
            ))
            .unwrap()
            .state,
        ModelChapterWorkflowAuthorityState::Staged {
            source_generation: 81
        }
    );
}

#[test]
fn discard_refuses_not_started_and_authoritative_cutovers() {
    let not_started = Fixture::new();
    assert_eq!(
        discard(&not_started, 82),
        Err(StorageError::ChapterWorkflowConflict)
    );

    let authoritative = Fixture::new();
    authoritative
        .store
        .stage_legacy_model_chapter_workflow_cutover(cutover(
            82,
            vec![entry(
                &authoritative,
                authoritative.request(82),
                LegacyModelChapterWorkflowDisposition::Ambiguous,
            )],
        ))
        .unwrap();
    authoritative
        .store
        .commit_legacy_model_chapter_workflow_cutover(82, NOW + 1)
        .unwrap();
    assert_eq!(
        discard(&authoritative, 82),
        Err(StorageError::ChapterWorkflowConflict)
    );
    assert_eq!(
        authoritative
            .store
            .model_chapter_workflow_authority()
            .unwrap(),
        ModelChapterWorkflowAuthorityState::Authoritative {
            source_generation: 82
        }
    );
    assert!(
        authoritative
            .store
            .model_chapter_workflow(authoritative.episode_id)
            .unwrap()
            .is_some()
    );
}

#[test]
fn discard_preserves_unattributed_workflow_state_and_the_stage_marker() {
    let fixture = Fixture::new();
    fixture
        .store
        .stage_legacy_model_chapter_workflow_cutover(cutover(83, Vec::new()))
        .unwrap();
    let unrelated = fixture.ensure(83, None);

    assert_eq!(
        discard(&fixture, 83),
        Err(StorageError::ChapterWorkflowConflict)
    );
    assert_eq!(
        fixture.store.model_chapter_workflow_authority().unwrap(),
        ModelChapterWorkflowAuthorityState::Staged {
            source_generation: 83
        }
    );
    assert_eq!(
        fixture
            .store
            .model_chapter_workflow(fixture.episode_id)
            .unwrap(),
        Some(unrelated)
    );
}
