use pod0_domain::{CancellationId, CommandId, HostRequestId, PodcastId, StateRevision};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoredFeedFetchIntent {
    Metadata,
    Refresh,
    Ensure,
    Subscribe,
}

impl StoredFeedFetchIntent {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Subscribe => "subscribe",
            Self::Ensure => "ensure",
            Self::Refresh => "refresh",
            Self::Metadata => "metadata",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "subscribe" => Some(Self::Subscribe),
            "ensure" => Some(Self::Ensure),
            "refresh" => Some(Self::Refresh),
            "metadata" => Some(Self::Metadata),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredFeedFetchStage {
    Requested,
    RetryScheduled,
    Failed,
}

impl StoredFeedFetchStage {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "retry_scheduled" => Some(Self::RetryScheduled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Durable host-request record for one feed-fetch workflow, shaped like
/// `DownloadHostRequestRecord`: the stored row is the single source of truth
/// and the facade keeps only a cache of it while a request is outstanding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedFetchWorkflowRecord {
    pub feed_key: String,
    pub source_url: String,
    pub podcast_id: PodcastId,
    pub intent: StoredFeedFetchIntent,
    pub stage: StoredFeedFetchStage,
    pub attempt: u16,
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub command_fingerprint: String,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub deadline_at_ms: Option<i64>,
    pub not_before_ms: Option<i64>,
    pub entity_tag: Option<String>,
    pub last_modified: Option<String>,
    pub failure_code: Option<String>,
    pub updated_at_ms: i64,
}

pub struct FeedFetchEnsureInput {
    pub command_id: CommandId,
    pub command_fingerprint: String,
    pub cancellation_id: CancellationId,
    pub source_url: String,
    pub feed_key: String,
    pub podcast_id: PodcastId,
    pub placeholder_title: String,
    pub intent: StoredFeedFetchIntent,
    pub entity_tag: Option<String>,
    pub last_modified: Option<String>,
    pub issued_revision: StateRevision,
    pub now_ms: i64,
    pub deadline_at_ms: i64,
}

/// `record` is `None` only when the commanded work already completed (the
/// workflow row is gone) and no new fetch is owed.
pub struct FeedFetchEnsureOutcome {
    pub podcast_id: PodcastId,
    pub record: Option<FeedFetchWorkflowRecord>,
}

#[derive(Clone, Debug)]
pub enum FeedFetchLeasedObservationAction {
    Apply {
        parsed: pod0_application::ParsedPodcastFeed,
        entity_tag: Option<String>,
        last_modified: Option<String>,
    },
    NotModified {
        entity_tag: Option<String>,
        last_modified: Option<String>,
    },
    Fail {
        failure_code: String,
        retry_at_ms: Option<i64>,
        retry_deadline_at_ms: Option<i64>,
    },
    Cancel,
}

#[derive(Clone, Debug)]
pub struct FeedFetchObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: pod0_application::DurableFeedHostObservation,
    pub action: FeedFetchLeasedObservationAction,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
}

#[derive(Clone, Debug)]
pub struct FeedFetchObservationCommitOutcome {
    pub replayed: bool,
    pub workflow: Option<FeedFetchWorkflowRecord>,
}
