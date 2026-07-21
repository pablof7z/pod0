use rusqlite::Connection;

use crate::runtime_chapter_workflow_test_support::{set_source, workflows};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

pub(super) fn fixture_without_model_authority() -> PlaybackFixture {
    let fixture = PlaybackFixture::new_with_transcript(true);
    set_source(&fixture, None);
    let connection = Connection::open(&fixture.target).unwrap();
    connection
        .execute(
            "DELETE FROM pod0_domain_cutovers WHERE domain='model_chapter_workflows'",
            [],
        )
        .unwrap();
    fixture
}

pub(super) fn ensure(facade: &Pod0Facade, episode_id: EpisodeId, command: u64) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(100, command),
        cancellation_id: CancellationId::from_parts(101, command),
        expected_revision: None,
        command: ApplicationCommand::EnsureModelChapters {
            episode_id,
            configured_model: "ollama:llama3.2".into(),
        },
    });
}

#[test]
fn model_commands_and_rehydration_are_barred_until_cutover_is_authoritative() {
    let fixture = fixture_without_model_authority();
    let facade = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();

    ensure(&facade, fixture.episode_id, 1);
    assert!(facade.next_host_requests(64).is_empty());
    assert!(
        workflows(&facade, Some(fixture.episode_id))
            .model
            .is_empty()
    );
    assert_eq!(
        facade.model_chapter_cutover().stage,
        LegacyModelChapterCutoverStage::NotStarted
    );
}

#[test]
fn matching_legacy_ambiguity_stays_dormant_after_commit_and_ordinary_ensure() {
    let fixture = fixture_without_model_authority();
    let facade = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let ChapterModelPlan::Ready { request } =
        facade.plan_chapter_model_request(fixture.episode_id, "ollama:llama3.2".into())
    else {
        panic!("fixture must produce a model request")
    };
    let staged = facade.stage_legacy_model_chapter_cutover(
        55,
        "ollama:llama3.2".into(),
        vec![LegacyModelChapterCutoverCandidate {
            episode_id: fixture.episode_id,
            input_version: request.source_version,
            disposition: LegacyModelChapterCutoverDisposition::Ambiguous,
        }],
    );
    assert_eq!(staged.stage, LegacyModelChapterCutoverStage::Staged);
    assert_eq!(staged.adopted_ambiguous, 1);
    assert!(facade.next_host_requests(64).is_empty());
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).model[0].stage,
        ModelChapterWorkflowStage::Ambiguous
    );

    assert_eq!(
        facade.commit_legacy_model_chapter_cutover(55).stage,
        LegacyModelChapterCutoverStage::Authoritative
    );
    ensure(&facade, fixture.episode_id, 2);
    assert!(facade.next_host_requests(64).is_empty());
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).model[0].stage,
        ModelChapterWorkflowStage::Ambiguous
    );
}

#[test]
fn stale_legacy_candidate_is_not_copied_and_rust_plans_current_work_after_commit() {
    let fixture = fixture_without_model_authority();
    let facade = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let staged = facade.stage_legacy_model_chapter_cutover(
        56,
        "ollama:llama3.2".into(),
        vec![LegacyModelChapterCutoverCandidate {
            episode_id: fixture.episode_id,
            input_version: "stale-input-version".into(),
            disposition: LegacyModelChapterCutoverDisposition::Ambiguous,
        }],
    );
    assert_eq!(staged.stage, LegacyModelChapterCutoverStage::Staged);
    assert_eq!(staged.adopted_ambiguous, 0);
    assert!(
        workflows(&facade, Some(fixture.episode_id))
            .model
            .is_empty()
    );
    assert_eq!(
        facade.commit_legacy_model_chapter_cutover(56).stage,
        LegacyModelChapterCutoverStage::Authoritative
    );

    ensure(&facade, fixture.episode_id, 3);
    assert!(
        facade
            .next_host_requests(64)
            .iter()
            .any(|request| matches!(request.request, HostRequest::ExecuteChapterModel { .. }))
    );
}

#[test]
fn transient_core_unavailability_cannot_silently_drop_legacy_candidates() {
    let fixture = fixture_without_model_authority();
    let facade = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    facade.state().transcript_store = None;

    let staged = facade.stage_legacy_model_chapter_cutover(
        60,
        "ollama:llama3.2".into(),
        vec![LegacyModelChapterCutoverCandidate {
            episode_id: fixture.episode_id,
            input_version: "current-source-cannot-be-read".into(),
            disposition: LegacyModelChapterCutoverDisposition::Ambiguous,
        }],
    );

    assert_eq!(staged.stage, LegacyModelChapterCutoverStage::Blocked);
    assert_eq!(
        facade.model_chapter_cutover().stage,
        LegacyModelChapterCutoverStage::NotStarted
    );
    assert!(
        workflows(&facade, Some(fixture.episode_id))
            .model
            .is_empty()
    );
}
