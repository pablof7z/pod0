use pod0_domain::{
    CancellationId, CommandId, DomainEventId, EpisodeId, HostRequestId, PodcastId, StateRevision,
    UnixTimestampMilliseconds,
};

use crate::OperationStage;

pub const MAX_FEED_RESPONSE_BYTES: u64 = 8 * 1_024 * 1_024;

#[must_use]
pub fn bounded_host_request_count(requested: u16) -> usize {
    usize::from(requested.clamp(1, crate::MAX_HOST_REQUEST_BATCH))
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DomainEventEnvelope {
    pub event_id: DomainEventId,
    pub state_revision: StateRevision,
    pub caused_by: CommandId,
    pub committed_at: UnixTimestampMilliseconds,
    pub event: DomainEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DomainEvent {
    CommandAccepted,
    HostRequestIssued {
        request_id: HostRequestId,
    },
    HostObservationAccepted {
        request_id: HostRequestId,
    },
    SubscriptionCommitted {
        podcast_id: PodcastId,
    },
    ResumePositionCommitted {
        episode_id: EpisodeId,
        position_milliseconds: u64,
    },
    OperationFinished {
        stage: OperationStage,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HostRequestEnvelope {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub request: HostRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostRequest {
    FetchFeed {
        feed_url: String,
        maximum_response_bytes: u64,
    },
    StartPlayback {
        episode_id: EpisodeId,
        audio_url: String,
        start_position_milliseconds: u64,
    },
    StopPlayback {
        episode_id: EpisodeId,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HostObservationEnvelope {
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
    pub observed_request_revision: StateRevision,
    pub observation: HostObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostObservation {
    FeedBytesFetched {
        bytes: Vec<u8>,
        entity_tag: Option<String>,
        last_modified: Option<String>,
    },
    PlaybackStarted {
        episode_id: EpisodeId,
        actual_position_milliseconds: u64,
    },
    PlaybackStopped {
        episode_id: EpisodeId,
        actual_position_milliseconds: u64,
        reason: PlaybackStopReason,
    },
    Failed {
        code: HostFailureCode,
        safe_detail: Option<String>,
    },
    Cancelled,
    Unsupported {
        wire_code: u32,
    },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostFailureCode {
    Offline,
    TimedOut,
    PermissionDenied,
    InvalidResponse,
    MediaUnavailable,
    PlatformFailure,
    Unsupported { wire_code: u32 },
}
