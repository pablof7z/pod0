use pod0_domain::{
    CancellationId, CommandId, DownloadAttemptId, DownloadIntentId, EpisodeId, HostRequestId,
    StateRevision,
};

use crate::StorageError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredDownloadOrigin {
    User,
    Playback,
    Automatic,
    Unsupported(u32),
}

impl StoredDownloadOrigin {
    pub(crate) const fn wire(self) -> (i64, Option<i64>) {
        match self {
            Self::User => (1, None),
            Self::Playback => (2, None),
            Self::Automatic => (3, None),
            Self::Unsupported(code) => (255, Some(code as i64)),
        }
    }

    pub(crate) fn parse(code: i64, wire: Option<i64>) -> Option<Self> {
        match code {
            1 => Some(Self::User),
            2 => Some(Self::Playback),
            3 => Some(Self::Automatic),
            255 => Some(Self::Unsupported(u32::try_from(wire?).ok()?)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredDownloadNetwork {
    Unknown,
    Unavailable,
    Wifi,
    Other,
    Unsupported(u32),
}

impl StoredDownloadNetwork {
    pub(crate) const fn wire(self) -> (i64, Option<i64>) {
        match self {
            Self::Unknown => (1, None),
            Self::Unavailable => (2, None),
            Self::Wifi => (3, None),
            Self::Other => (4, None),
            Self::Unsupported(code) => (255, Some(code as i64)),
        }
    }

    pub(crate) fn parse(code: i64, wire: Option<i64>) -> Option<Self> {
        match code {
            1 => Some(Self::Unknown),
            2 => Some(Self::Unavailable),
            3 => Some(Self::Wifi),
            4 => Some(Self::Other),
            255 => Some(Self::Unsupported(u32::try_from(wire?).ok()?)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredDownloadDesiredState {
    Present,
    Absent,
}

impl StoredDownloadDesiredState {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "present" => Some(Self::Present),
            "absent" => Some(Self::Absent),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredDownloadStage {
    Waiting,
    Requested,
    HostAccepted,
    Transferring,
    Staged,
    RetryScheduled,
    Removing,
    Cancelled,
    Failed,
    Succeeded,
}

impl StoredDownloadStage {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Requested => "requested",
            Self::HostAccepted => "host_accepted",
            Self::Transferring => "transferring",
            Self::Staged => "staged",
            Self::RetryScheduled => "retry_scheduled",
            Self::Removing => "removing",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Succeeded => "succeeded",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "waiting" => Some(Self::Waiting),
            "requested" => Some(Self::Requested),
            "host_accepted" => Some(Self::HostAccepted),
            "transferring" => Some(Self::Transferring),
            "staged" => Some(Self::Staged),
            "retry_scheduled" => Some(Self::RetryScheduled),
            "removing" => Some(Self::Removing),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            "succeeded" => Some(Self::Succeeded),
            _ => None,
        }
    }

    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Requested
                | Self::HostAccepted
                | Self::Transferring
                | Self::Staged
                | Self::RetryScheduled
                | Self::Removing
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadEnvironmentRecord {
    pub network: StoredDownloadNetwork,
    pub available_capacity_bytes: Option<u64>,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadWorkflowRecord {
    pub episode_id: EpisodeId,
    pub intent_id: DownloadIntentId,
    pub input_version: String,
    pub origin: StoredDownloadOrigin,
    pub desired_state: StoredDownloadDesiredState,
    pub stage: StoredDownloadStage,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub attempt_id: Option<DownloadAttemptId>,
    pub request_id: Option<HostRequestId>,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub deadline_at_ms: Option<i64>,
    pub not_before_ms: Option<i64>,
    pub enclosure_url: String,
    pub resume_key: Option<String>,
    pub external_task_key: Option<String>,
    pub artifact_key: Option<String>,
    pub artifact_byte_count: Option<u64>,
    pub artifact_digest: Option<[u8; 32]>,
    pub failure_code: Option<String>,
    pub failure_detail: Option<String>,
    pub failure_retryable: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadHostRequestKind {
    Start,
    Cancel,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadHostRequestRecord {
    pub request_id: HostRequestId,
    pub episode_id: EpisodeId,
    pub kind: DownloadHostRequestKind,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub deadline_at_ms: Option<i64>,
    pub intent_id: Option<DownloadIntentId>,
    pub attempt_id: Option<DownloadAttemptId>,
    pub input_version: Option<String>,
    pub enclosure_url: Option<String>,
    pub resume_key: Option<String>,
    pub external_task_key: Option<String>,
    pub artifact_key: Option<String>,
    pub last_sequence_number: Option<u64>,
}

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
