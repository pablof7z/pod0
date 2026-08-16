use super::*;

#[derive(Clone, Copy)]
pub(crate) struct FixedClock(pub i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

pub(crate) fn dispatch(facade: &Pod0Facade, id: u64, command: ApplicationCommand) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(70, id),
        cancellation_id: CancellationId::from_parts(71, id),
        expected_revision: None,
        command,
    });
}

pub(crate) fn workflows(facade: &Pod0Facade, episode_id: EpisodeId) -> DownloadWorkflowsProjection {
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

pub(crate) fn observe_wifi(facade: &Pod0Facade, id: u64) {
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

pub(crate) fn request_download(fixture: &PlaybackFixture, id: u64) {
    dispatch(
        &fixture.facade,
        id,
        ApplicationCommand::RequestEpisodeDownload {
            episode_id: fixture.episode_id,
            origin: DownloadIntentOrigin::User,
        },
    );
}

pub(crate) fn staged_observation(
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

pub(crate) fn leased_observation(
    request: &LeasedHostRequestEnvelope,
    sequence: u64,
    observation: HostObservation,
) -> LeasedHostObservationEnvelope {
    LeasedHostObservationEnvelope {
        lease: request.lease,
        observation: HostObservationEnvelope {
            request_id: request.request.request_id,
            cancellation_id: request.request.cancellation_id,
            observed_request_revision: request.request.issued_revision,
            sequence_number: sequence,
            observed_at: request.lease.expires_at,
            observation,
        },
    }
}

pub(crate) fn leased_staged_observation(
    request: &LeasedHostRequestEnvelope,
    sequence: u64,
    path: String,
    byte_count: u64,
) -> LeasedHostObservationEnvelope {
    leased_observation(
        request,
        sequence,
        staged_observation(&request.request, sequence, path, byte_count).observation,
    )
}
