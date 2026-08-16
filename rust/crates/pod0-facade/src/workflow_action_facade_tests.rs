use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

fn dispatch(facade: &Pod0Facade, id: u64, command: ApplicationCommand) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(70, id),
        cancellation_id: CancellationId::from_parts(71, id),
        expected_revision: None,
        command,
    });
}

fn workflow(fixture: &PlaybackFixture) -> DownloadWorkflowProjection {
    let Projection::Downloads { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Downloads {
                episode_id: Some(fixture.episode_id),
            },
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected downloads projection")
    };
    value.workflows.into_iter().next().unwrap()
}

#[test]
fn workflow_projection_action_token_is_exact_and_stale_safe() {
    let fixture = PlaybackFixture::new();
    dispatch(
        &fixture.facade,
        901,
        ApplicationCommand::ObserveDownloadEnvironment {
            observation: DownloadEnvironmentObservation {
                network: DownloadNetworkState::Wifi,
                available_capacity_bytes: Some(2_000_000_000),
            },
        },
    );
    dispatch(
        &fixture.facade,
        902,
        ApplicationCommand::RequestEpisodeDownload {
            episode_id: fixture.episode_id,
            origin: DownloadIntentOrigin::User,
        },
    );
    let cancel = workflow(&fixture)
        .cancel_action
        .expect("Rust projects exact cancel token");

    let mut forged = cancel;
    forged.authorization = ContentDigest::from_bytes([99; 32]);
    assert_eq!(
        fixture.facade.execute_workflow_action(forged),
        WorkflowActionDispatchResult::InvalidToken
    );
    assert_ne!(workflow(&fixture).stage, DownloadWorkflowStage::Cancelled);

    assert_eq!(
        fixture.facade.execute_workflow_action(cancel),
        WorkflowActionDispatchResult::Accepted
    );
    assert_eq!(workflow(&fixture).stage, DownloadWorkflowStage::Cancelled);
    assert_eq!(
        fixture.facade.execute_workflow_action(cancel),
        WorkflowActionDispatchResult::Stale
    );
}
