use pod0_domain::{
    CancellationId, CommandId, ContentDigest, EpisodeId, HostRequestId, StateRevision,
    TranscriptArtifactId, TranscriptVersionId,
};

use super::model::{PreparedTranscriptAttempt, StoredTranscriptWorkflowRequest};

pub const MAX_LEGACY_TRANSCRIPT_WORKFLOW_ROWS: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptWorkflowAuthorityState {
    NotStarted,
    Staged { source_generation: u64 },
    Verified { source_generation: u64 },
    Authoritative { source_generation: u64 },
}

impl TranscriptWorkflowAuthorityState {
    pub const fn is_authoritative(self) -> bool {
        matches!(self, Self::Authoritative { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyTranscriptWorkflowRowClassification {
    Restart,
    RecoverProvider,
    Ambiguous,
    Blocked,
    Failed,
    Cancelled,
    Succeeded,
    IndexPending,
    IndexSucceeded,
    Obsolete,
}

impl LegacyTranscriptWorkflowRowClassification {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Restart => "restart",
            Self::RecoverProvider => "recover_provider",
            Self::Ambiguous => "ambiguous",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Succeeded => "succeeded",
            Self::IndexPending => "index_pending",
            Self::IndexSucceeded => "index_succeeded",
            Self::Obsolete => "obsolete",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "restart" => Self::Restart,
            "recover_provider" => Self::RecoverProvider,
            "ambiguous" => Self::Ambiguous,
            "blocked" => Self::Blocked,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "succeeded" => Self::Succeeded,
            "index_pending" => Self::IndexPending,
            "index_succeeded" => Self::IndexSucceeded,
            "obsolete" => Self::Obsolete,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTranscriptWorkflowBackupRow {
    pub episode_id: EpisodeId,
    pub row_bytes: Vec<u8>,
    pub row_fingerprint: ContentDigest,
    pub classification: LegacyTranscriptWorkflowRowClassification,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyTranscriptWorkflowDisposition {
    Restart,
    RecoverProvider {
        external_operation_id: String,
        provider_status: Option<String>,
    },
    Ambiguous,
    Blocked {
        failure_code: String,
        failure_detail: Option<String>,
        may_have_submitted: bool,
    },
    Failed {
        failure_code: String,
        failure_detail: Option<String>,
        may_have_submitted: bool,
    },
    Cancelled {
        may_have_submitted: bool,
    },
    Succeeded {
        artifact_id: TranscriptArtifactId,
        transcript_version_id: TranscriptVersionId,
        content_digest: ContentDigest,
        selection_revision: StateRevision,
    },
    IndexPending {
        artifact_id: TranscriptArtifactId,
        transcript_version_id: TranscriptVersionId,
        content_digest: ContentDigest,
        selection_revision: StateRevision,
        evidence_input_version: String,
    },
    IndexSucceeded {
        artifact_id: TranscriptArtifactId,
        transcript_version_id: TranscriptVersionId,
        content_digest: ContentDigest,
        selection_revision: StateRevision,
        evidence_input_version: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTranscriptWorkflowCandidate {
    pub episode_id: EpisodeId,
    pub request: StoredTranscriptWorkflowRequest,
    pub request_id: Option<HostRequestId>,
    pub prepared_attempt: Option<PreparedTranscriptAttempt>,
    pub deadline_at_ms: Option<i64>,
    pub expected_selection_revision: StateRevision,
    pub disposition: LegacyTranscriptWorkflowDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTranscriptWorkflowCutoverInput {
    pub source_generation: u64,
    pub source_fingerprint: ContentDigest,
    pub backup_digest: ContentDigest,
    pub backup_byte_count: u64,
    pub rows: Vec<LegacyTranscriptWorkflowBackupRow>,
    pub candidates: Vec<LegacyTranscriptWorkflowCandidate>,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub max_attempts: u16,
    pub now_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTranscriptWorkflowCutoverReport {
    pub state: TranscriptWorkflowAuthorityState,
    pub source_fingerprint: ContentDigest,
    pub row_count: u32,
    pub adopted_workflow_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowRollbackExport {
    pub source_generation: u64,
    pub source_fingerprint: ContentDigest,
    pub backup_digest: ContentDigest,
    pub backup_byte_count: u64,
    pub rows: Vec<LegacyTranscriptWorkflowBackupRow>,
}
