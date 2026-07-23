use pod0_domain::ContentDigest;
use pod0_storage::{AgentHistoryCutoverState, LegacyAgentHistoryCutoverReport, StorageError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyAgentHistoryCutoverStage {
    NotStarted,
    Staged,
    Verified,
    Authoritative,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyAgentHistoryCutoverFailureCode {
    InvalidSource,
    ConflictingCoreState,
    StorageUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyAgentHistoryCutoverFailure {
    pub code: LegacyAgentHistoryCutoverFailureCode,
    pub diagnostic_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyAgentHistoryCutoverProjection {
    pub stage: LegacyAgentHistoryCutoverStage,
    pub source_generation: Option<u64>,
    pub source_fingerprint: Option<ContentDigest>,
    pub backup_digest: Option<ContentDigest>,
    pub backup_byte_count: Option<u64>,
    pub conversation_count: u32,
    pub turn_count: u32,
    pub message_count: u32,
    pub failure: Option<LegacyAgentHistoryCutoverFailure>,
}

impl LegacyAgentHistoryCutoverProjection {
    pub(super) fn from_report(report: LegacyAgentHistoryCutoverReport) -> Self {
        let (stage, source_generation) = match report.state {
            AgentHistoryCutoverState::NotStarted => {
                (LegacyAgentHistoryCutoverStage::NotStarted, None)
            }
            AgentHistoryCutoverState::Staged { source_generation } => (
                LegacyAgentHistoryCutoverStage::Staged,
                Some(source_generation),
            ),
            AgentHistoryCutoverState::Verified { source_generation } => (
                LegacyAgentHistoryCutoverStage::Verified,
                Some(source_generation),
            ),
            AgentHistoryCutoverState::Authoritative { source_generation } => (
                LegacyAgentHistoryCutoverStage::Authoritative,
                Some(source_generation),
            ),
        };
        Self {
            stage,
            source_generation,
            source_fingerprint: report.source_fingerprint,
            backup_digest: report.backup_digest,
            backup_byte_count: report.backup_byte_count,
            conversation_count: report.conversation_count,
            turn_count: report.turn_count,
            message_count: report.message_count,
            failure: None,
        }
    }

    pub(super) fn inspected(
        source_generation: u64,
        source_fingerprint: ContentDigest,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        conversation_count: usize,
        turn_count: usize,
        message_count: usize,
    ) -> Self {
        Self {
            stage: LegacyAgentHistoryCutoverStage::NotStarted,
            source_generation: Some(source_generation),
            source_fingerprint: Some(source_fingerprint),
            backup_digest: Some(backup_digest),
            backup_byte_count: Some(backup_byte_count),
            conversation_count: conversation_count as u32,
            turn_count: turn_count as u32,
            message_count: message_count as u32,
            failure: None,
        }
    }

    pub(super) fn blocked(error: StorageError) -> Self {
        let code = match error {
            StorageError::InvalidLegacyRecord { .. } | StorageError::ImportLimitExceeded { .. } => {
                LegacyAgentHistoryCutoverFailureCode::InvalidSource
            }
            StorageError::AgentTurnConflict => {
                LegacyAgentHistoryCutoverFailureCode::ConflictingCoreState
            }
            _ => LegacyAgentHistoryCutoverFailureCode::StorageUnavailable,
        };
        Self {
            stage: LegacyAgentHistoryCutoverStage::Blocked,
            source_generation: None,
            source_fingerprint: None,
            backup_digest: None,
            backup_byte_count: None,
            conversation_count: 0,
            turn_count: 0,
            message_count: 0,
            failure: Some(LegacyAgentHistoryCutoverFailure {
                code,
                diagnostic_code: error.code().to_owned(),
            }),
        }
    }
}
