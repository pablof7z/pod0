use pod0_domain::{
    CancellationId, CommandId, EpisodeId, FeedDiscoveryOccurrenceId, HostRequestId, PodcastId,
    StateRevision,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedDiscoveryEffectKind {
    Download,
    Notification,
}

impl FeedDiscoveryEffectKind {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::Notification => "notification",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedDiscoveryEffectStage {
    Pending,
    Requested,
    RetryScheduled,
    Succeeded,
    Obsolete,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedDiscoveryEffectRecord {
    pub occurrence_id: FeedDiscoveryOccurrenceId,
    pub podcast_id: PodcastId,
    pub episode_id: EpisodeId,
    pub podcast_title: String,
    pub episode_title: String,
    pub kind: FeedDiscoveryEffectKind,
    pub stage: FeedDiscoveryEffectStage,
    pub command_id: Option<CommandId>,
    pub cancellation_id: CancellationId,
    pub request_id: Option<HostRequestId>,
    pub attempt: u8,
    pub not_before_ms: Option<i64>,
    pub deadline_at_ms: Option<i64>,
    pub expires_at_ms: i64,
    pub workflow_revision: StateRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedDiscoveryNotificationOutcome {
    Delivered,
    RetryableFailure,
    PermissionDenied,
    Cancelled,
    PermanentFailure,
}

#[derive(Clone, Debug)]
pub struct FeedNotificationObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: pod0_application::DurableFeedHostObservation,
    pub outcome: FeedDiscoveryNotificationOutcome,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
}

#[derive(Clone, Debug)]
pub struct FeedNotificationObservationCommitOutcome {
    pub replayed: bool,
    pub effect: Option<FeedDiscoveryEffectRecord>,
}
