use super::*;

pub(super) fn map_state(state: PlaybackState) -> PlaybackHostState {
    match state {
        PlaybackState::Idle => PlaybackHostState::Idle,
        PlaybackState::Loading => PlaybackHostState::Loading,
        PlaybackState::Prepared => PlaybackHostState::Prepared,
        PlaybackState::Playing => PlaybackHostState::Playing,
        PlaybackState::Paused => PlaybackHostState::Paused,
        PlaybackState::Ended => PlaybackHostState::Idle,
        PlaybackState::Failed => PlaybackHostState::Failed,
    }
}

pub(super) fn idle_observation(episode_id: Option<EpisodeId>) -> PlaybackLifecycleObservation {
    PlaybackLifecycleObservation {
        episode_id,
        state: PlaybackHostState::Idle,
        position_milliseconds: 0,
        duration_milliseconds: 0,
        route: PlaybackAudioRoute::BuiltIn,
        interruption: PlaybackInterruption::None,
        ended: false,
    }
}

pub(super) fn failed_state(episode_id: EpisodeId) -> HostObservation {
    HostObservation::PlaybackObserved {
        value: failed_observation_value(Some(episode_id)),
    }
}

pub(super) fn failed_observation(
    episode_id: Option<EpisodeId>,
    _error: &MediaError,
) -> HostObservation {
    // The durable playback contract carries no free-text detail on
    // PlaybackObserved; the Failed state signals the failure. The error text is
    // intentionally not fabricated into another observation kind (no mocks).
    HostObservation::PlaybackObserved {
        value: failed_observation_value(episode_id),
    }
}

pub(super) fn failed_observation_value(
    episode_id: Option<EpisodeId>,
) -> PlaybackLifecycleObservation {
    PlaybackLifecycleObservation {
        episode_id,
        state: PlaybackHostState::Failed,
        position_milliseconds: 0,
        duration_milliseconds: 0,
        route: PlaybackAudioRoute::BuiltIn,
        interruption: PlaybackInterruption::None,
        ended: false,
    }
}
