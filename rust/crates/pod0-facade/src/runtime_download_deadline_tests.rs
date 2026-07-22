use std::sync::Arc;

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

fn dispatch(facade: &Pod0Facade, id: u64, command: ApplicationCommand) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(80, id),
        cancellation_id: CancellationId::from_parts(81, id),
        expected_revision: None,
        command,
    });
}

#[test]
fn expired_request_is_fenced_and_retried_only_after_policy_delay() {
    let fixture = PlaybackFixture::new();
    let issued_at = 1_800_000_000_000;
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(issued_at)));
    dispatch(
        &fixture.facade,
        1,
        ApplicationCommand::ObserveDownloadEnvironment {
            observation: DownloadEnvironmentObservation {
                network: DownloadNetworkState::Wifi,
                available_capacity_bytes: None,
            },
        },
    );
    dispatch(
        &fixture.facade,
        2,
        ApplicationCommand::RequestEpisodeDownload {
            episode_id: fixture.episode_id,
            origin: DownloadIntentOrigin::User,
        },
    );

    let expired_at = issued_at + pod0_application::DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS;
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(expired_at)));
    assert!(fixture.facade.next_host_requests(20).is_empty());
    let Projection::Downloads { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Downloads {
                episode_id: Some(fixture.episode_id),
            },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected download projection")
    };
    assert_eq!(
        value.workflows[0].stage,
        DownloadWorkflowStage::RetryScheduled
    );
    assert_eq!(value.workflows[0].attempt, 2);
    let Projection::Library { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected library projection")
    };
    assert!(value.operations.iter().any(|operation| {
        operation.command_id == CommandId::from_parts(80, 2)
            && operation.stage == OperationStage::Running
            && operation.failure.is_none()
    }));

    fixture.facade.state().set_clock(Arc::new(FixedClock(
        expired_at + pod0_application::DOWNLOAD_RETRY_DELAY_MILLISECONDS,
    )));
    let retry = fixture.facade.next_host_requests(20).pop().unwrap();
    assert!(matches!(
        retry.request,
        HostRequest::StartEpisodeDownload { .. }
    ));
}
