use crate::model_chapter_cutover_tests::{ensure, fixture_without_model_authority};
use crate::runtime_chapter_workflow_test_support::workflows;
use crate::*;

fn retry(
    facade: &Pod0Facade,
    episode_id: EpisodeId,
    expected_workflow_revision: StateRevision,
    command: u64,
) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(102, command),
        cancellation_id: CancellationId::from_parts(103, command),
        expected_revision: None,
        command: ApplicationCommand::RetryModelChapters {
            episode_id,
            configured_model: "ollama:llama3.2".into(),
            expected_workflow_revision,
        },
    });
}

fn cancel(
    facade: &Pod0Facade,
    episode_id: EpisodeId,
    expected_workflow_revision: StateRevision,
    command: u64,
) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(104, command),
        cancellation_id: CancellationId::from_parts(105, command),
        expected_revision: None,
        command: ApplicationCommand::CancelModelChapters {
            episode_id,
            expected_workflow_revision,
        },
    });
}

fn assert_host_unavailable(facade: &Pod0Facade, command_id: CommandId) {
    let Projection::Library { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected library projection")
    };
    assert!(matches!(
        value
            .operations
            .iter()
            .find(|operation| operation.command_id == command_id),
        Some(OperationProjection {
            stage: OperationStage::Failed,
            failure: Some(CoreFailure {
                code: CoreFailureCode::HostUnavailable,
                ..
            }),
            ..
        })
    ));
}

#[test]
fn staged_cutover_is_inert_and_can_only_be_discarded_by_exact_generation() {
    let fixture = fixture_without_model_authority();
    let facade = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let ChapterModelPlan::Ready { request } =
        facade.plan_chapter_model_request(fixture.episode_id, "ollama:llama3.2".into())
    else {
        panic!("fixture must produce a model request")
    };
    let source_version = request.source_version;
    let candidate = LegacyModelChapterCutoverCandidate {
        episode_id: fixture.episode_id,
        input_version: source_version.clone(),
        disposition: LegacyModelChapterCutoverDisposition::Ambiguous,
    };
    assert_eq!(
        facade
            .stage_legacy_model_chapter_cutover(
                57,
                "ollama:llama3.2".into(),
                vec![candidate.clone()],
            )
            .stage,
        LegacyModelChapterCutoverStage::Staged
    );
    let staged = workflows(&facade, Some(fixture.episode_id)).model[0].clone();

    ensure(&facade, fixture.episode_id, 10);
    retry(&facade, fixture.episode_id, staged.workflow_revision, 11);
    cancel(&facade, fixture.episode_id, staged.workflow_revision, 12);
    assert_host_unavailable(&facade, CommandId::from_parts(100, 10));
    assert_host_unavailable(&facade, CommandId::from_parts(102, 11));
    assert_host_unavailable(&facade, CommandId::from_parts(104, 12));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).model,
        vec![staged]
    );
    assert!(facade.next_host_requests(64).is_empty());

    assert_eq!(
        facade.discard_staged_legacy_model_chapter_cutover(58).stage,
        LegacyModelChapterCutoverStage::Blocked
    );
    assert_eq!(
        facade.model_chapter_cutover().stage,
        LegacyModelChapterCutoverStage::Staged
    );
    assert_eq!(
        facade.discard_staged_legacy_model_chapter_cutover(57).stage,
        LegacyModelChapterCutoverStage::NotStarted
    );
    assert!(
        workflows(&facade, Some(fixture.episode_id))
            .model
            .is_empty()
    );
    assert_eq!(
        facade
            .stage_legacy_model_chapter_cutover(
                58,
                "ollama:llama3.2".into(),
                vec![LegacyModelChapterCutoverCandidate {
                    episode_id: fixture.episode_id,
                    input_version: source_version,
                    disposition: LegacyModelChapterCutoverDisposition::Ambiguous,
                }],
            )
            .stage,
        LegacyModelChapterCutoverStage::Staged
    );
}
