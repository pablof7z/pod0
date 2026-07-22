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

fn observe_wifi(facade: &Pod0Facade, id: u64) {
    dispatch(
        facade,
        id,
        ApplicationCommand::ObserveDownloadEnvironment {
            observation: DownloadEnvironmentObservation {
                network: DownloadNetworkState::Wifi,
                available_capacity_bytes: Some(2_000_000_000),
            },
        },
    );
}

fn request_download(fixture: &PlaybackFixture, id: u64) {
    dispatch(
        &fixture.facade,
        id,
        ApplicationCommand::RequestEpisodeDownload {
            episode_id: fixture.episode_id,
            origin: DownloadIntentOrigin::User,
        },
    );
}

fn staged_observation(
    request: &HostRequestEnvelope,
    sequence: u64,
    path: String,
    byte_count: u64,
) -> HostObservationEnvelope {
    let HostRequest::StartEpisodeDownload {
        episode_id,
        intent_id,
        attempt_id,
        ..
    } = request.request
    else {
        panic!("expected start download")
    };
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: sequence,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_000_100),
        observation: HostObservation::DownloadStaged {
            episode_id,
            intent_id,
            attempt_id,
            staged_file_path: path,
            byte_count,
        },
    }
}

#[test]
fn staged_host_file_becomes_durable_episode_state_and_projection() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    observe_wifi(&fixture.facade, 1);
    request_download(&fixture, 2);
    let request = fixture.facade.next_host_requests(20).pop().unwrap();
    let media = fixture.target.parent().unwrap().join("native-staged.media");
    let bytes = b"facade durable media";
    std::fs::write(&media, bytes).unwrap();

    let receipt = fixture.facade.record_host_observation(staged_observation(
        &request,
        1,
        media.to_string_lossy().into_owned(),
        bytes.len() as u64,
    ));
    assert_eq!(
        receipt,
        HostObservationReceipt::Persisted {
            request_id: request.request_id,
            terminal: true,
        }
    );
    assert_eq!(
        workflows(&fixture.facade, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::Succeeded
    );
    let Projection::EpisodeDetail { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::EpisodeDetail {
                episode_id: fixture.episode_id,
            },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected episode projection")
    };
    assert!(matches!(
        value.episode.unwrap().download,
        DownloadArtifactStatus::Available { byte_count, .. } if byte_count == bytes.len() as u64
    ));

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        workflows(&reopened, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::Succeeded
    );
    assert!(reopened.next_host_requests(20).is_empty());
}

#[test]
fn cancellation_fences_late_start_completion_and_emits_durable_cancel_request() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    observe_wifi(&fixture.facade, 1);
    request_download(&fixture, 2);
    let start = fixture.facade.next_host_requests(20).pop().unwrap();
    let projection = workflows(&fixture.facade, fixture.episode_id)
        .workflows
        .remove(0);
    dispatch(
        &fixture.facade,
        3,
        ApplicationCommand::CancelEpisodeDownload {
            episode_id: fixture.episode_id,
            expected_workflow_revision: projection.workflow_revision,
        },
    );
    let cancel = fixture.facade.next_host_requests(20).pop().unwrap();
    assert!(matches!(
        cancel.request,
        HostRequest::CancelEpisodeDownload { .. }
    ));

    let media = fixture.target.parent().unwrap().join("late.media");
    std::fs::write(&media, b"late").unwrap();
    assert!(matches!(
        fixture.facade.record_host_observation(staged_observation(
            &start,
            1,
            media.to_string_lossy().into_owned(),
            4,
        )),
        HostObservationReceipt::Rejected {
            reason: HostObservationRejection::UnknownRequest,
            ..
        } | HostObservationReceipt::Rejected {
            reason: HostObservationRejection::Cancelled,
            ..
        }
    ));
    assert_eq!(
        workflows(&fixture.facade, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::Cancelled
    );

    let HostRequest::CancelEpisodeDownload {
        episode_id,
        intent_id,
        attempt_id,
        ..
    } = cancel.request
    else {
        panic!("expected cancellation")
    };
    assert_eq!(
        fixture
            .facade
            .record_host_observation(HostObservationEnvelope {
                request_id: cancel.request_id,
                cancellation_id: cancel.cancellation_id,
                observed_request_revision: cancel.issued_revision,
                sequence_number: 1,
                observed_at: UnixTimestampMilliseconds::new(1_800_000_000_200),
                observation: HostObservation::DownloadCancelled {
                    episode_id,
                    intent_id,
                    attempt_id
                },
            }),
        HostObservationReceipt::Persisted {
            request_id: cancel.request_id,
            terminal: true
        }
    );
}

#[test]
fn retry_request_is_not_emitted_until_kernel_owned_deadline() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    observe_wifi(&fixture.facade, 1);
    request_download(&fixture, 2);
    let request = fixture.facade.next_host_requests(20).pop().unwrap();
    let receipt = fixture
        .facade
        .record_host_observation(HostObservationEnvelope {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 1,
            observed_at: UnixTimestampMilliseconds::new(1_800_000_000_100),
            observation: HostObservation::Failed {
                code: HostFailureCode::Offline,
                safe_detail: None,
            },
        });
    assert!(matches!(
        receipt,
        HostObservationReceipt::Persisted { terminal: true, .. }
    ));
    assert!(fixture.facade.next_host_requests(20).is_empty());
    assert_eq!(
        workflows(&fixture.facade, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::RetryScheduled
    );

    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_300_100)));
    let retry = fixture.facade.next_host_requests(20).pop().unwrap();
    assert_ne!(retry.request_id, request.request_id);
    assert!(matches!(
        retry.request,
        HostRequest::StartEpisodeDownload { .. }
    ));
}
