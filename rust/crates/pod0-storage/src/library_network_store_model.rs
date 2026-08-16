use pod0_application::{
    LibraryNetworkIntent, LibraryNetworkStep, PodcastDirectoryEntry, ResolvedSharedEpisode,
};
use pod0_domain::{CancellationId, CommandId, EpisodeId, HostRequestId, StateRevision};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StoredLibraryNetworkResult {
    Directory {
        entries: Vec<PodcastDirectoryEntry>,
    },
    SharedEpisode {
        episode_id: EpisodeId,
    },
    Catalog {
        episode_ids: Vec<EpisodeId>,
        bounded_result: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredLibraryNetworkStage {
    Requested,
    AwaitingFollowup,
    Completed,
    Failed,
    Cancelled,
}

impl StoredLibraryNetworkStage {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::AwaitingFollowup => "awaiting_followup",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "awaiting_followup" => Some(Self::AwaitingFollowup),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryNetworkWorkflowRecord {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub command_fingerprint: String,
    pub intent: LibraryNetworkIntent,
    pub stage: StoredLibraryNetworkStage,
    pub revision: StateRevision,
    pub pending_request_id: Option<HostRequestId>,
    pub pending_step: Option<LibraryNetworkStep>,
    pub result: Option<StoredLibraryNetworkResult>,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct LibraryNetworkAdmissionInput {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub command_fingerprint: String,
    pub fingerprint: pod0_domain::ContentDigest,
    pub intent: LibraryNetworkIntent,
    pub now_ms: i64,
    pub deadline_at_ms: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum LibraryNetworkObservationAction {
    CompleteDirectory {
        results: Vec<PodcastDirectoryEntry>,
    },
    ContinueTopLookup {
        ranked_ids: Vec<u64>,
        request: pod0_application::LibraryHttpRequest,
    },
    CompleteTopLookup {
        results: Vec<PodcastDirectoryEntry>,
    },
    ContinueShared {
        step: LibraryNetworkStep,
        request: pod0_application::LibraryHttpRequest,
    },
    CompleteShared {
        episode: ResolvedSharedEpisode,
    },
    ContinueCatalog {
        step: LibraryNetworkStep,
        request: pod0_application::LibraryHttpRequest,
    },
    CompleteCatalog {
        candidates: Vec<pod0_application::CatalogEpisodeCandidate>,
    },
    Fail {
        code: String,
    },
    Cancel,
}

#[derive(Clone, Debug)]
pub struct LibraryNetworkObservationInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
    pub observed_request_revision: StateRevision,
    pub sequence_number: u64,
    pub observation: pod0_application::LibraryDocumentObservation,
    pub action: LibraryNetworkObservationAction,
    pub observed_at_ms: i64,
}
