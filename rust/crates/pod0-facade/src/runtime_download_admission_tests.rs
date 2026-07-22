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
        command_id: CommandId::from_parts(70, id),
        cancellation_id: CancellationId::from_parts(71, id),
        expected_revision: None,
        command,
    });
}

fn workflows(facade: &Pod0Facade, episode_id: EpisodeId) -> DownloadWorkflowsProjection {
    let Projection::Downloads { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Downloads {
                episode_id: Some(episode_id),
            },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected download projection")
    };
    value
}

#[test]
fn waiting_request_is_admitted_by_environment_and_survives_restart() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    dispatch(
        &fixture.facade,
        1,
        ApplicationCommand::RequestEpisodeDownload {
            episode_id: fixture.episode_id,
            origin: DownloadIntentOrigin::User,
        },
    );
    let waiting = workflows(&fixture.facade, fixture.episode_id);
    assert_eq!(waiting.failure, None);
    assert_eq!(
        waiting.workflows[0].stage,
        DownloadWorkflowStage::WaitingForEnvironment
    );
    assert!(fixture.facade.next_host_requests(20).is_empty());

    dispatch(
        &fixture.facade,
        2,
        ApplicationCommand::ObserveDownloadEnvironment {
            observation: DownloadEnvironmentObservation {
                network: DownloadNetworkState::Wifi,
                available_capacity_bytes: Some(2_000_000_000),
            },
        },
    );
    let request = fixture.facade.next_host_requests(20).pop().unwrap();
    assert!(matches!(
        request.request,
        HostRequest::StartEpisodeDownload { .. }
    ));

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let recovered = reopened.next_host_requests(20).pop().unwrap();
    assert_eq!(recovered, request);
    assert!(reopened.next_host_requests(20).is_empty());
}

#[test]
fn automatic_waiting_request_is_retired_when_policy_becomes_obsolete() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    dispatch(
        &fixture.facade,
        1,
        ApplicationCommand::SetSubscriptionAutoDownload {
            podcast_id: fixture.podcast_id,
            policy: AutoDownloadPolicy {
                mode: AutoDownloadMode::AllNew,
                wifi_only: false,
            },
        },
    );
    dispatch(
        &fixture.facade,
        2,
        ApplicationCommand::RequestEpisodeDownload {
            episode_id: fixture.episode_id,
            origin: DownloadIntentOrigin::Automatic,
        },
    );
    assert_eq!(
        workflows(&fixture.facade, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::WaitingForEnvironment
    );

    dispatch(
        &fixture.facade,
        3,
        ApplicationCommand::SetSubscriptionAutoDownload {
            podcast_id: fixture.podcast_id,
            policy: AutoDownloadPolicy {
                mode: AutoDownloadMode::Off,
                wifi_only: false,
            },
        },
    );

    let retired = workflows(&fixture.facade, fixture.episode_id)
        .workflows
        .remove(0);
    assert_eq!(retired.stage, DownloadWorkflowStage::Cancelled);
    assert_eq!(retired.desired_state, DownloadDesiredState::Absent);
    assert!(fixture.facade.next_host_requests(20).is_empty());
}
