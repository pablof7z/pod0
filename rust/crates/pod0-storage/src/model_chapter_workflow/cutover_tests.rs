use pod0_domain::{CancellationId, CommandId, ContentDigest, StateRevision};

use super::cutover_test_support::{
    NOW, activate_chapter_authority, cutover, entry, qualified_artifact,
};
use super::tests::Fixture;
use super::*;
use crate::StorageError;

#[test]
fn empty_cutover_is_restart_safe_and_becomes_authoritative_once() {
    let fixture = Fixture::new();
    let staged = fixture
        .store
        .stage_legacy_model_chapter_workflow_cutover(cutover(7, Vec::new()))
        .unwrap();
    assert_eq!(
        staged.state,
        ModelChapterWorkflowAuthorityState::Staged {
            source_generation: 7
        }
    );
    assert_eq!(
        fixture
            .store
            .stage_legacy_model_chapter_workflow_cutover(cutover(7, Vec::new()))
            .unwrap(),
        staged
    );
    let authoritative = fixture
        .store
        .commit_legacy_model_chapter_workflow_cutover(7, NOW + 1)
        .unwrap();
    assert_eq!(
        authoritative,
        ModelChapterWorkflowAuthorityState::Authoritative {
            source_generation: 7
        }
    );
    assert_eq!(
        fixture
            .store
            .commit_legacy_model_chapter_workflow_cutover(7, NOW + 2)
            .unwrap(),
        authoritative
    );
}

#[test]
fn ambiguous_legacy_attempt_never_becomes_implicitly_dispatchable() {
    let fixture = Fixture::new();
    let request = fixture.request(31);
    fixture
        .store
        .stage_legacy_model_chapter_workflow_cutover(cutover(
            8,
            vec![entry(
                &fixture,
                request.clone(),
                LegacyModelChapterWorkflowDisposition::Ambiguous,
            )],
        ))
        .unwrap();
    fixture
        .store
        .commit_legacy_model_chapter_workflow_cutover(8, NOW + 1)
        .unwrap();

    let migrated = fixture
        .store
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(migrated.state, ModelChapterWorkflowState::Ambiguous);
    assert!(migrated.may_have_submitted);
    assert!(
        fixture
            .store
            .dispatchable_model_chapter_workflows(10)
            .unwrap()
            .is_empty()
    );
    let repeated = fixture
        .store
        .ensure_model_chapter_workflow(ModelChapterEnsureInput {
            episode_id: fixture.episode_id,
            configured_model: request.configured_model.clone(),
            desired_plan: ModelChapterDesiredPlan::Ready(Box::new(request)),
            command_id: CommandId::from_parts(50, 1),
            cancellation_id: CancellationId::from_parts(51, 1),
            issued_revision: StateRevision::new(50),
            now_ms: NOW + 2,
            request_deadline_ms: NOW + 60_002,
            max_attempts: 8,
            force_retry_from_revision: None,
        })
        .unwrap();
    assert_eq!(repeated, ModelChapterEnsureOutcome::Existing(migrated));
}

#[test]
fn terminal_legacy_states_keep_the_same_request_fingerprint_dormant() {
    let cases = [
        (
            LegacyModelChapterWorkflowDisposition::Blocked {
                failure_code: "missing_credential".into(),
                failure_detail: Some("Credential unavailable".into()),
                may_have_submitted: false,
            },
            ModelChapterWorkflowState::Blocked,
            false,
        ),
        (
            LegacyModelChapterWorkflowDisposition::Failed {
                failure_code: "retry_exhausted".into(),
                failure_detail: None,
                may_have_submitted: true,
            },
            ModelChapterWorkflowState::Failed,
            true,
        ),
        (
            LegacyModelChapterWorkflowDisposition::Cancelled {
                may_have_submitted: true,
            },
            ModelChapterWorkflowState::Cancelled,
            true,
        ),
    ];
    for (index, (disposition, expected_state, may_have_submitted)) in cases.into_iter().enumerate()
    {
        let fixture = Fixture::new();
        fixture
            .store
            .stage_legacy_model_chapter_workflow_cutover(cutover(
                20 + index as u64,
                vec![entry(&fixture, fixture.request(40), disposition)],
            ))
            .unwrap();
        let record = fixture
            .store
            .model_chapter_workflow(fixture.episode_id)
            .unwrap()
            .unwrap();
        assert_eq!(record.state, expected_state);
        assert_eq!(record.may_have_submitted, may_have_submitted);
    }
}

#[test]
fn exact_legacy_success_adopts_selected_artifact_and_forgery_rolls_back() {
    let fixture = Fixture::new();
    activate_chapter_authority(&fixture);
    let mut request = fixture.request(61);
    let artifact = qualified_artifact(&fixture, &request);
    let receipt = fixture
        .store
        .commit_and_select_chapter(
            CommandId::from_parts(60, 1),
            StateRevision::INITIAL,
            artifact,
            NOW,
        )
        .unwrap();
    request.expected_selection_revision = receipt.selection_revision;
    let mut disposition = LegacyModelChapterWorkflowDisposition::Succeeded {
        artifact_id: receipt.artifact_id,
        content_digest: receipt.content_digest,
        integrity_digest: receipt.integrity_digest,
        selection_revision: receipt.selection_revision,
    };
    if let LegacyModelChapterWorkflowDisposition::Succeeded { content_digest, .. } =
        &mut disposition
    {
        *content_digest = ContentDigest::from_bytes([0xff; 32]);
    }
    assert_eq!(
        fixture
            .store
            .stage_legacy_model_chapter_workflow_cutover(cutover(
                61,
                vec![entry(&fixture, request.clone(), disposition)],
            )),
        Err(StorageError::ChapterWorkflowConflict)
    );
    assert_eq!(
        fixture.store.model_chapter_workflow_authority().unwrap(),
        ModelChapterWorkflowAuthorityState::NotStarted
    );

    let valid = LegacyModelChapterWorkflowDisposition::Succeeded {
        artifact_id: receipt.artifact_id,
        content_digest: receipt.content_digest,
        integrity_digest: receipt.integrity_digest,
        selection_revision: receipt.selection_revision,
    };
    fixture
        .store
        .stage_legacy_model_chapter_workflow_cutover(cutover(
            62,
            vec![entry(&fixture, request, valid)],
        ))
        .unwrap();
    let adopted = fixture
        .store
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(adopted.state, ModelChapterWorkflowState::Succeeded);
    assert_eq!(adopted.selected_artifact_id, Some(receipt.artifact_id));
}

#[test]
fn cutover_refuses_to_overwrite_preexisting_rust_workflow_state() {
    let fixture = Fixture::new();
    fixture.ensure(70, None);
    assert_eq!(
        fixture
            .store
            .stage_legacy_model_chapter_workflow_cutover(cutover(70, Vec::new())),
        Err(StorageError::ChapterWorkflowConflict)
    );
}
