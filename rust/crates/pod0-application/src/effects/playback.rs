use pod0_domain::EpisodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackTransitionCue {
    Immediate,
    FadeIn { duration_milliseconds: u32 },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NativeTimerMode {
    Duration { duration_milliseconds: u64 },
    EndOfEpisode,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlaybackLifecycleObservation {
    pub episode_id: Option<EpisodeId>,
    pub state: PlaybackHostState,
    pub position_milliseconds: u64,
    pub duration_milliseconds: u64,
    pub route: PlaybackAudioRoute,
    pub interruption: PlaybackInterruption,
    pub ended: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackHostState {
    Idle,
    Loading,
    Prepared,
    Playing,
    Paused,
    Buffering,
    Failed,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackAudioRoute {
    BuiltIn,
    Wired,
    Bluetooth,
    AirPlay,
    Car,
    External,
    Unknown,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackInterruption {
    None,
    Began,
    EndedShouldResume,
    EndedShouldRemainPaused,
    RouteLost,
    MediaServicesReset,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackStopReason {
    UserInitiated,
    ReachedEnd,
    AudioRouteLost,
    Interrupted,
    HostFailure,
    Unsupported { wire_code: u32 },
}
