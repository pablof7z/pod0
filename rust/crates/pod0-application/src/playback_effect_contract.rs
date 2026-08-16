use pod0_domain::{
    CancellationId, ChapterArtifactId, ChapterPlaybackSessionId, CommandId, EpisodeId,
    HostRequestId, StateRevision, UnixTimestampMilliseconds,
};

use crate::{
    ChapterPlaybackContext, HostRequest, HostRequestEnvelope, NativeTimerMode,
    PlaybackTransitionCue,
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurablePlaybackEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub deadline_at: Option<UnixTimestampMilliseconds>,
    pub action: DurablePlaybackEffectAction,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurablePlaybackEffectAction {
    LoadMedia {
        episode_id: EpisodeId,
        audio_url: String,
        start_position_milliseconds: u64,
    },
    Play {
        episode_id: EpisodeId,
        transition_cue: DurablePlaybackTransitionCue,
    },
    Pause {
        episode_id: EpisodeId,
    },
    Seek {
        episode_id: EpisodeId,
        position_milliseconds: u64,
        reason: DurablePlaybackSeekReason,
        chapter_context: Option<DurableChapterPlaybackContext>,
    },
    SetRate {
        episode_id: EpisodeId,
        rate_permille: u16,
    },
    ArmNativeTimer {
        episode_id: EpisodeId,
        mode: DurableNativeTimerMode,
    },
    CancelNativeTimer {
        episode_id: EpisodeId,
    },
    ObservePlayback {
        episode_id: Option<EpisodeId>,
        minimum_interval_milliseconds: u32,
    },
    StopPlayback {
        episode_id: EpisodeId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurablePlaybackTransitionCue {
    Immediate,
    FadeIn { duration_milliseconds: u32 },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurablePlaybackSeekReason {
    UserRequested,
    NextChapter,
    PreviousChapter,
    PreviousChapterRestart,
    AutomaticAdSkip,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableNativeTimerMode {
    Duration { duration_milliseconds: u64 },
    EndOfEpisode,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableChapterPlaybackContext {
    pub episode_id: EpisodeId,
    pub artifact_id: ChapterArtifactId,
    pub selection_revision: StateRevision,
    pub session_id: ChapterPlaybackSessionId,
    pub policy_version: u32,
}

impl DurablePlaybackEffectRequest {
    pub fn from_host(request: HostRequestEnvelope) -> Option<Self> {
        let action = DurablePlaybackEffectAction::from_host(request.request)?;
        Some(Self {
            request_id: request.request_id,
            command_id: request.command_id,
            cancellation_id: request.cancellation_id,
            issued_revision: request.issued_revision,
            deadline_at: request.deadline_at,
            action,
        })
    }

    #[must_use]
    pub fn to_host(&self) -> HostRequestEnvelope {
        HostRequestEnvelope {
            request_id: self.request_id,
            command_id: self.command_id,
            cancellation_id: self.cancellation_id,
            issued_revision: self.issued_revision,
            deadline_at: self.deadline_at,
            request: self.action.to_host(),
        }
    }

    #[must_use]
    pub const fn episode_id(&self) -> Option<EpisodeId> {
        self.action.episode_id()
    }
}

include!("playback_effect_contract_mapping.rs");
