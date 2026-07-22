use pod0_domain::{
    CommandId, DomainEventId, EpisodeId, HostRequestId, PodcastId, StateRevision,
    TranscriptArtifactId, TranscriptVersionId, UnixTimestampMilliseconds,
};

use crate::OperationStage;

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
    TranscriptArtifactCommitted {
        episode_id: EpisodeId,
        artifact_id: TranscriptArtifactId,
        transcript_version_id: TranscriptVersionId,
    },
    TranscriptSelectionChanged {
        episode_id: EpisodeId,
        artifact_id: TranscriptArtifactId,
        selection_revision: StateRevision,
    },
    OperationFinished {
        stage: OperationStage,
    },
    Unsupported {
        wire_code: u32,
    },
}
