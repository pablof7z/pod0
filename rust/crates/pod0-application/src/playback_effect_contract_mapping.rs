impl DurablePlaybackEffectAction {
    fn from_host(request: HostRequest) -> Option<Self> {
        Some(match request {
            HostRequest::LoadMedia { episode_id, audio_url, start_position_milliseconds } => Self::LoadMedia { episode_id, audio_url, start_position_milliseconds },
            HostRequest::Play { episode_id, transition_cue } => Self::Play { episode_id, transition_cue: transition_cue.into() },
            HostRequest::Pause { episode_id } => Self::Pause { episode_id },
            HostRequest::Seek { episode_id, position_milliseconds, reason, chapter_context } => Self::Seek {
                episode_id,
                position_milliseconds,
                reason: match reason {
                    pod0_domain::PlaybackSeekReason::UserRequested => DurablePlaybackSeekReason::UserRequested,
                    pod0_domain::PlaybackSeekReason::NextChapter => DurablePlaybackSeekReason::NextChapter,
                    pod0_domain::PlaybackSeekReason::PreviousChapter => DurablePlaybackSeekReason::PreviousChapter,
                    pod0_domain::PlaybackSeekReason::PreviousChapterRestart => DurablePlaybackSeekReason::PreviousChapterRestart,
                    pod0_domain::PlaybackSeekReason::AutomaticAdSkip => DurablePlaybackSeekReason::AutomaticAdSkip,
                    pod0_domain::PlaybackSeekReason::Unsupported { wire_code } => DurablePlaybackSeekReason::Unsupported { wire_code },
                },
                chapter_context: chapter_context.map(Into::into),
            },
            HostRequest::SetRate { episode_id, rate } => Self::SetRate { episode_id, rate_permille: rate.value },
            HostRequest::ArmNativeTimer { episode_id, mode } => Self::ArmNativeTimer { episode_id, mode: mode.into() },
            HostRequest::CancelNativeTimer { episode_id } => Self::CancelNativeTimer { episode_id },
            HostRequest::ObservePlayback { episode_id, minimum_interval_milliseconds } => Self::ObservePlayback { episode_id, minimum_interval_milliseconds },
            HostRequest::StopPlayback { episode_id } => Self::StopPlayback { episode_id },
            _ => return None,
        })
    }

    fn to_host(&self) -> HostRequest {
        match self {
            Self::LoadMedia { episode_id, audio_url, start_position_milliseconds } => HostRequest::LoadMedia { episode_id: *episode_id, audio_url: audio_url.clone(), start_position_milliseconds: *start_position_milliseconds },
            Self::Play { episode_id, transition_cue } => HostRequest::Play { episode_id: *episode_id, transition_cue: (*transition_cue).into() },
            Self::Pause { episode_id } => HostRequest::Pause { episode_id: *episode_id },
            Self::Seek { episode_id, position_milliseconds, reason, chapter_context } => HostRequest::Seek {
                episode_id: *episode_id,
                position_milliseconds: *position_milliseconds,
                reason: match reason {
                    DurablePlaybackSeekReason::UserRequested => pod0_domain::PlaybackSeekReason::UserRequested,
                    DurablePlaybackSeekReason::NextChapter => pod0_domain::PlaybackSeekReason::NextChapter,
                    DurablePlaybackSeekReason::PreviousChapter => pod0_domain::PlaybackSeekReason::PreviousChapter,
                    DurablePlaybackSeekReason::PreviousChapterRestart => pod0_domain::PlaybackSeekReason::PreviousChapterRestart,
                    DurablePlaybackSeekReason::AutomaticAdSkip => pod0_domain::PlaybackSeekReason::AutomaticAdSkip,
                    DurablePlaybackSeekReason::Unsupported { wire_code } => pod0_domain::PlaybackSeekReason::Unsupported { wire_code: *wire_code },
                },
                chapter_context: chapter_context.map(Into::into),
            },
            Self::SetRate { episode_id, rate_permille } => HostRequest::SetRate { episode_id: *episode_id, rate: pod0_domain::PlaybackRatePermille { value: *rate_permille } },
            Self::ArmNativeTimer { episode_id, mode } => HostRequest::ArmNativeTimer { episode_id: *episode_id, mode: (*mode).into() },
            Self::CancelNativeTimer { episode_id } => HostRequest::CancelNativeTimer { episode_id: *episode_id },
            Self::ObservePlayback { episode_id, minimum_interval_milliseconds } => HostRequest::ObservePlayback { episode_id: *episode_id, minimum_interval_milliseconds: *minimum_interval_milliseconds },
            Self::StopPlayback { episode_id } => HostRequest::StopPlayback { episode_id: *episode_id },
        }
    }

    const fn episode_id(&self) -> Option<EpisodeId> {
        match self {
            Self::LoadMedia { episode_id, .. } | Self::Play { episode_id, .. } | Self::Pause { episode_id } | Self::Seek { episode_id, .. } | Self::SetRate { episode_id, .. } | Self::ArmNativeTimer { episode_id, .. } | Self::CancelNativeTimer { episode_id } | Self::StopPlayback { episode_id } => Some(*episode_id),
            Self::ObservePlayback { episode_id, .. } => *episode_id,
        }
    }
}

impl From<PlaybackTransitionCue> for DurablePlaybackTransitionCue {
    fn from(value: PlaybackTransitionCue) -> Self { match value { PlaybackTransitionCue::Immediate => Self::Immediate, PlaybackTransitionCue::FadeIn { duration_milliseconds } => Self::FadeIn { duration_milliseconds }, PlaybackTransitionCue::Unsupported { wire_code } => Self::Unsupported { wire_code } } }
}
impl From<DurablePlaybackTransitionCue> for PlaybackTransitionCue {
    fn from(value: DurablePlaybackTransitionCue) -> Self { match value { DurablePlaybackTransitionCue::Immediate => Self::Immediate, DurablePlaybackTransitionCue::FadeIn { duration_milliseconds } => Self::FadeIn { duration_milliseconds }, DurablePlaybackTransitionCue::Unsupported { wire_code } => Self::Unsupported { wire_code } } }
}
impl From<NativeTimerMode> for DurableNativeTimerMode {
    fn from(value: NativeTimerMode) -> Self { match value { NativeTimerMode::Duration { duration_milliseconds } => Self::Duration { duration_milliseconds }, NativeTimerMode::EndOfEpisode => Self::EndOfEpisode, NativeTimerMode::Unsupported { wire_code } => Self::Unsupported { wire_code } } }
}
impl From<DurableNativeTimerMode> for NativeTimerMode {
    fn from(value: DurableNativeTimerMode) -> Self { match value { DurableNativeTimerMode::Duration { duration_milliseconds } => Self::Duration { duration_milliseconds }, DurableNativeTimerMode::EndOfEpisode => Self::EndOfEpisode, DurableNativeTimerMode::Unsupported { wire_code } => Self::Unsupported { wire_code } } }
}
impl From<ChapterPlaybackContext> for DurableChapterPlaybackContext {
    fn from(value: ChapterPlaybackContext) -> Self { Self { episode_id: value.episode_id, artifact_id: value.artifact_id, selection_revision: value.selection_revision, session_id: value.session_id, policy_version: value.policy_version } }
}
impl From<DurableChapterPlaybackContext> for ChapterPlaybackContext {
    fn from(value: DurableChapterPlaybackContext) -> Self { Self { episode_id: value.episode_id, artifact_id: value.artifact_id, selection_revision: value.selection_revision, session_id: value.session_id, policy_version: value.policy_version } }
}
