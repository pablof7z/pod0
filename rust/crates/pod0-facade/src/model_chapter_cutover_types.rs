use pod0_domain::{ChapterArtifactId, ContentDigest, EpisodeId, StateRevision};
use pod0_storage::{ModelChapterWorkflowAuthorityState, StorageError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyModelChapterCutoverStage {
    NotStarted,
    Staged,
    Authoritative,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyModelChapterCutoverFailureCode {
    InvalidSource,
    ConflictingCoreState,
    StorageUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyModelChapterCutoverFailure {
    pub code: LegacyModelChapterCutoverFailureCode,
    pub diagnostic_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyModelChapterCutoverDisposition {
    Succeeded {
        artifact_id: ChapterArtifactId,
        content_digest: ContentDigest,
        integrity_digest: ContentDigest,
        selection_revision: StateRevision,
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
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyModelChapterCutoverCandidate {
    pub episode_id: EpisodeId,
    pub input_version: String,
    pub disposition: LegacyModelChapterCutoverDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyModelChapterCutoverProjection {
    pub stage: LegacyModelChapterCutoverStage,
    pub source_generation: Option<u64>,
    pub adopted_succeeded: u32,
    pub adopted_ambiguous: u32,
    pub failure: Option<LegacyModelChapterCutoverFailure>,
}

impl LegacyModelChapterCutoverProjection {
    pub(super) fn from_authority(state: ModelChapterWorkflowAuthorityState) -> Self {
        match state {
            ModelChapterWorkflowAuthorityState::NotStarted => Self {
                stage: LegacyModelChapterCutoverStage::NotStarted,
                source_generation: None,
                adopted_succeeded: 0,
                adopted_ambiguous: 0,
                failure: None,
            },
            ModelChapterWorkflowAuthorityState::Staged { source_generation } => Self {
                stage: LegacyModelChapterCutoverStage::Staged,
                source_generation: Some(source_generation),
                adopted_succeeded: 0,
                adopted_ambiguous: 0,
                failure: None,
            },
            ModelChapterWorkflowAuthorityState::Authoritative { source_generation } => Self {
                stage: LegacyModelChapterCutoverStage::Authoritative,
                source_generation: Some(source_generation),
                adopted_succeeded: 0,
                adopted_ambiguous: 0,
                failure: None,
            },
        }
    }

    pub(super) fn blocked(error: StorageError) -> Self {
        let code = match error {
            StorageError::ChapterWorkflowConflict => {
                LegacyModelChapterCutoverFailureCode::ConflictingCoreState
            }
            StorageError::InvalidChapterArtifact | StorageError::InvalidTranscriptArtifact => {
                LegacyModelChapterCutoverFailureCode::InvalidSource
            }
            _ => LegacyModelChapterCutoverFailureCode::StorageUnavailable,
        };
        Self {
            stage: LegacyModelChapterCutoverStage::Blocked,
            source_generation: None,
            adopted_succeeded: 0,
            adopted_ambiguous: 0,
            failure: Some(LegacyModelChapterCutoverFailure {
                code,
                diagnostic_code: error.code().to_owned(),
            }),
        }
    }
}
