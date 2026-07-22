use pod0_application::DownloadIntentOrigin;
use pod0_domain::EpisodeId;
use pod0_storage::{DownloadWorkflowAuthorityState, LegacyDownloadCutoverReport, StorageError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyDownloadCutoverStage {
    NotStarted,
    Staged,
    Authoritative,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyDownloadCutoverFailureCode {
    InvalidSource,
    ConflictingCoreState,
    StorageUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyDownloadCutoverFailure {
    pub code: LegacyDownloadCutoverFailureCode,
    pub diagnostic_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyDownloadCutoverDisposition {
    Available {
        source_path: String,
        byte_count: u64,
    },
    Restart {
        resume_available: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyDownloadCutoverCandidate {
    pub episode_id: EpisodeId,
    pub origin: DownloadIntentOrigin,
    pub disposition: LegacyDownloadCutoverDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyDownloadCutoverProjection {
    pub stage: LegacyDownloadCutoverStage,
    pub source_generation: Option<u64>,
    pub adopted_available: u32,
    pub scheduled_restart: u32,
    pub repaired_invalid: u32,
    pub failure: Option<LegacyDownloadCutoverFailure>,
}

impl LegacyDownloadCutoverProjection {
    pub(super) fn from_report(report: LegacyDownloadCutoverReport) -> Self {
        let (stage, source_generation) = stage(report.state);
        Self {
            stage,
            source_generation,
            adopted_available: report.adopted_available,
            scheduled_restart: report.scheduled_restart,
            repaired_invalid: report.repaired_invalid,
            failure: None,
        }
    }

    pub(super) fn from_authority(state: DownloadWorkflowAuthorityState) -> Self {
        let (stage, source_generation) = stage(state);
        Self {
            stage,
            source_generation,
            adopted_available: 0,
            scheduled_restart: 0,
            repaired_invalid: 0,
            failure: None,
        }
    }

    pub(super) fn blocked(error: StorageError) -> Self {
        let code = match error {
            StorageError::InvalidDownloadArtifact => {
                LegacyDownloadCutoverFailureCode::InvalidSource
            }
            StorageError::DownloadWorkflowConflict => {
                LegacyDownloadCutoverFailureCode::ConflictingCoreState
            }
            _ => LegacyDownloadCutoverFailureCode::StorageUnavailable,
        };
        Self {
            stage: LegacyDownloadCutoverStage::Blocked,
            source_generation: None,
            adopted_available: 0,
            scheduled_restart: 0,
            repaired_invalid: 0,
            failure: Some(LegacyDownloadCutoverFailure {
                code,
                diagnostic_code: error.code().to_owned(),
            }),
        }
    }
}

fn stage(state: DownloadWorkflowAuthorityState) -> (LegacyDownloadCutoverStage, Option<u64>) {
    match state {
        DownloadWorkflowAuthorityState::NotStarted => {
            (LegacyDownloadCutoverStage::NotStarted, None)
        }
        DownloadWorkflowAuthorityState::Staged { source_generation } => {
            (LegacyDownloadCutoverStage::Staged, Some(source_generation))
        }
        DownloadWorkflowAuthorityState::Authoritative { source_generation } => (
            LegacyDownloadCutoverStage::Authoritative,
            Some(source_generation),
        ),
    }
}
