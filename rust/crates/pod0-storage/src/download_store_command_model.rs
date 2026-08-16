use pod0_domain::{
    CancellationId, CommandId, DownloadIntentId, EpisodeId, HostRequestId, StateRevision,
};

use crate::{DownloadWorkflowRecord, StorageError, StoredDownloadOrigin};

#[derive(Clone, Debug)]
pub struct DownloadEnsureInput {
    pub episode_id: EpisodeId,
    pub intent_id: DownloadIntentId,
    pub input_version: String,
    pub origin: StoredDownloadOrigin,
    pub admitted: bool,
    pub wait_failure_code: Option<String>,
    pub command_id: CommandId,
    pub command_fingerprint: String,
    pub cancellation_id: CancellationId,
    pub enclosure_url: String,
    pub issued_revision: StateRevision,
    pub now_ms: i64,
    pub deadline_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct DownloadRemovalInput {
    pub command_id: CommandId,
    pub command_fingerprint: String,
    pub episode_id: EpisodeId,
    pub expected_revision: StateRevision,
    pub issued_revision: StateRevision,
    pub now_ms: i64,
    pub deadline_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadEnsureOutcome {
    Changed {
        record: DownloadWorkflowRecord,
        replaced: Option<Box<DownloadWorkflowRecord>>,
    },
    Existing(DownloadWorkflowRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadWorkflowPage {
    pub items: Vec<DownloadWorkflowRecord>,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct DownloadFailureInput {
    pub request_id: HostRequestId,
    pub sequence_number: u64,
    pub failure_code: String,
    pub failure_detail: Option<String>,
    pub retryable: bool,
    pub retry_at_ms: Option<i64>,
    pub retry_deadline_at_ms: Option<i64>,
    pub issued_revision: StateRevision,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadObservationOutcome {
    Updated(DownloadWorkflowRecord),
    Duplicate(DownloadWorkflowRecord),
    Stale,
}

#[derive(Clone, Debug)]
pub enum DownloadLeasedObservationAction {
    Accepted {
        external_task_key: String,
        resume_key: Option<String>,
    },
    Cancellation,
    Removal {
        artifact_key: String,
    },
    Staged {
        staged_file_path: String,
        claimed_byte_count: u64,
    },
    Failure(DownloadFailureInput),
}

#[derive(Clone, Debug)]
pub struct DownloadObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: pod0_application::HostObservationEnvelope,
    pub action: DownloadLeasedObservationAction,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
}

#[derive(Clone, Debug)]
pub struct DownloadObservationCommitOutcome {
    pub workflow: DownloadWorkflowRecord,
    pub replayed: bool,
    pub terminal_effect: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadWorkflowTransition {
    pub record: DownloadWorkflowRecord,
    pub replaced: Option<Box<DownloadWorkflowRecord>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DownloadRecoveryReport {
    pub adopted_count: u16,
    pub repaired_count: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadArtifactBoundary {
    AfterStagedRecord,
    AfterArtifactRename,
}

pub trait DownloadArtifactObserver {
    fn reached(&self, boundary: DownloadArtifactBoundary) -> Result<(), StorageError>;
}
