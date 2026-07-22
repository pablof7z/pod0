use pod0_application::{TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin};
use pod0_domain::{ContentDigest, EpisodeId};
use pod0_storage::{
    LegacyTranscriptWorkflowCutoverReport, StorageError, TranscriptWorkflowAuthorityState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyTranscriptWorkflowCutoverStage {
    NotStarted,
    Staged,
    Verified,
    Authoritative,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyTranscriptWorkflowCutoverFailureCode {
    InvalidSource,
    ConflictingCoreState,
    StorageUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptWorkflowCutoverFailure {
    pub code: LegacyTranscriptWorkflowCutoverFailureCode,
    pub diagnostic_code: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
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

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptWorkflowBackupRow {
    pub episode_id: EpisodeId,
    pub row_bytes: Vec<u8>,
    pub row_fingerprint: ContentDigest,
    pub classification: LegacyTranscriptWorkflowRowClassification,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyTranscriptWorkflowCutoverDisposition {
    Restart {
        attempt: u16,
    },
    RecoverProvider {
        attempt: u16,
        external_operation_id: String,
        provider_status: Option<String>,
    },
    Ambiguous {
        attempt: u16,
    },
    Blocked {
        attempt: Option<u16>,
        failure_code: String,
        failure_detail: Option<String>,
        may_have_submitted: bool,
    },
    Failed {
        attempt: Option<u16>,
        failure_code: String,
        failure_detail: Option<String>,
        may_have_submitted: bool,
    },
    Cancelled {
        attempt: Option<u16>,
        may_have_submitted: bool,
    },
    Succeeded {
        attempt: Option<u16>,
    },
    IndexPending {
        evidence_input_version: String,
    },
    IndexSucceeded {
        evidence_input_version: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptWorkflowCutoverCandidate {
    pub episode_id: EpisodeId,
    pub source_revision: String,
    pub origin: TranscriptWorkflowOrigin,
    pub configuration: TranscriptWorkflowConfiguration,
    pub disposition: LegacyTranscriptWorkflowCutoverDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptWorkflowCutoverProjection {
    pub stage: LegacyTranscriptWorkflowCutoverStage,
    pub source_generation: Option<u64>,
    pub source_fingerprint: Option<ContentDigest>,
    pub row_count: u32,
    pub adopted_workflow_count: u32,
    pub failure: Option<LegacyTranscriptWorkflowCutoverFailure>,
}

impl LegacyTranscriptWorkflowCutoverProjection {
    pub(super) fn not_started() -> Self {
        Self {
            stage: LegacyTranscriptWorkflowCutoverStage::NotStarted,
            source_generation: None,
            source_fingerprint: None,
            row_count: 0,
            adopted_workflow_count: 0,
            failure: None,
        }
    }

    pub(super) fn from_report(report: LegacyTranscriptWorkflowCutoverReport) -> Self {
        let (stage, source_generation) = match report.state {
            TranscriptWorkflowAuthorityState::NotStarted => {
                (LegacyTranscriptWorkflowCutoverStage::NotStarted, None)
            }
            TranscriptWorkflowAuthorityState::Staged { source_generation } => (
                LegacyTranscriptWorkflowCutoverStage::Staged,
                Some(source_generation),
            ),
            TranscriptWorkflowAuthorityState::Verified { source_generation } => (
                LegacyTranscriptWorkflowCutoverStage::Verified,
                Some(source_generation),
            ),
            TranscriptWorkflowAuthorityState::Authoritative { source_generation } => (
                LegacyTranscriptWorkflowCutoverStage::Authoritative,
                Some(source_generation),
            ),
        };
        Self {
            stage,
            source_generation,
            source_fingerprint: Some(report.source_fingerprint),
            row_count: report.row_count,
            adopted_workflow_count: report.adopted_workflow_count,
            failure: None,
        }
    }

    pub(super) fn blocked(error: StorageError) -> Self {
        let code = match error {
            StorageError::TranscriptWorkflowConflict => {
                LegacyTranscriptWorkflowCutoverFailureCode::ConflictingCoreState
            }
            StorageError::InvalidTranscriptArtifact => {
                LegacyTranscriptWorkflowCutoverFailureCode::InvalidSource
            }
            _ => LegacyTranscriptWorkflowCutoverFailureCode::StorageUnavailable,
        };
        Self {
            stage: LegacyTranscriptWorkflowCutoverStage::Blocked,
            source_generation: None,
            source_fingerprint: None,
            row_count: 0,
            adopted_workflow_count: 0,
            failure: Some(LegacyTranscriptWorkflowCutoverFailure {
                code,
                diagnostic_code: error.code().to_owned(),
            }),
        }
    }
}
