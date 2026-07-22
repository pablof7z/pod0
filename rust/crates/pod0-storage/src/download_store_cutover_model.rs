use pod0_domain::{
    CancellationId, CommandId, DownloadAttemptId, DownloadIntentId, EpisodeId, HostRequestId,
    StateRevision,
};

use crate::StoredDownloadOrigin;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadWorkflowAuthorityState {
    NotStarted,
    Staged { source_generation: u64 },
    Authoritative { source_generation: u64 },
}

impl DownloadWorkflowAuthorityState {
    pub const fn is_authoritative(self) -> bool {
        matches!(self, Self::Authoritative { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyDownloadCutoverDisposition {
    Available {
        source_path: String,
        byte_count: u64,
    },
    Restart {
        resume_available: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyDownloadCutoverEntry {
    pub episode_id: EpisodeId,
    pub intent_id: DownloadIntentId,
    pub attempt_id: DownloadAttemptId,
    pub request_id: HostRequestId,
    pub input_version: String,
    pub enclosure_url: String,
    pub origin: StoredDownloadOrigin,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub disposition: LegacyDownloadCutoverDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyDownloadCutoverInput {
    pub source_generation: u64,
    pub entries: Vec<LegacyDownloadCutoverEntry>,
    pub issued_revision: StateRevision,
    pub now_ms: i64,
    pub deadline_at_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyDownloadCutoverReport {
    pub state: DownloadWorkflowAuthorityState,
    pub adopted_available: u32,
    pub scheduled_restart: u32,
    pub repaired_invalid: u32,
}
