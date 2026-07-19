use super::*;

pub(crate) fn record_playback(
    facade: &Pod0Facade,
    stream: &HostRequestEnvelope,
    sequence_number: u64,
    observed_at: i64,
    position: u64,
    ended: bool,
    interruption: PlaybackInterruption,
) {
    record_observation(
        facade,
        stream,
        sequence_number,
        observed_at,
        PlaybackLifecycleObservation {
            episode_id: playback(facade).current.map(|item| item.episode_id),
            state: if ended {
                PlaybackHostState::Paused
            } else {
                PlaybackHostState::Playing
            },
            position_milliseconds: position,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption,
            ended,
        },
    );
}

pub(crate) fn record_observation(
    facade: &Pod0Facade,
    request: &HostRequestEnvelope,
    sequence_number: u64,
    observed_at: i64,
    value: PlaybackLifecycleObservation,
) {
    facade.record_host_observation(HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number,
        observed_at: UnixTimestampMilliseconds::new(observed_at),
        observation: HostObservation::PlaybackObserved { value },
    });
}

pub(crate) fn library_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Library,
        offset: 0,
        max_items: 200,
    }
}

pub(crate) fn playback_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Playback,
        offset: 0,
        max_items: 200,
    }
}
