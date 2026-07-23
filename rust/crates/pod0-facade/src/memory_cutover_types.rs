use pod0_domain::{ContentDigest, MemoryId, UnixTimestampMilliseconds};
use pod0_storage::{LegacyMemoryCutoverReport, MemoryCutoverState, StorageError};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyMemoryInput {
    pub memory_id: MemoryId,
    pub content: String,
    pub created_at: UnixTimestampMilliseconds,
    pub deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyCompiledMemoryInput {
    pub text: String,
    pub compiled_at: UnixTimestampMilliseconds,
    pub source_memory_ids: Vec<MemoryId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyMemoryCutoverStage {
    NotStarted,
    Staged,
    Verified,
    Authoritative,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyMemoryCutoverFailureCode {
    InvalidSource,
    ConflictingCoreState,
    StorageUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyMemoryCutoverProjection {
    pub stage: LegacyMemoryCutoverStage,
    pub source_generation: Option<u64>,
    pub source_fingerprint: Option<ContentDigest>,
    pub backup_digest: Option<ContentDigest>,
    pub backup_byte_count: Option<u64>,
    pub memory_count: u32,
    pub deleted_count: u32,
    pub compiled_present: bool,
    pub failure: Option<LegacyMemoryCutoverFailureCode>,
}

impl LegacyMemoryCutoverProjection {
    pub(super) fn from_report(report: LegacyMemoryCutoverReport) -> Self {
        Self {
            stage: match report.state {
                MemoryCutoverState::NotStarted => LegacyMemoryCutoverStage::NotStarted,
                MemoryCutoverState::Staged { .. } => LegacyMemoryCutoverStage::Staged,
                MemoryCutoverState::Verified { .. } => LegacyMemoryCutoverStage::Verified,
                MemoryCutoverState::Authoritative { .. } => LegacyMemoryCutoverStage::Authoritative,
            },
            source_generation: report.state.source_generation(),
            source_fingerprint: report.source_fingerprint,
            backup_digest: report.backup_digest,
            backup_byte_count: report.backup_byte_count,
            memory_count: report.memory_count,
            deleted_count: report.deleted_count,
            compiled_present: report.compiled_present,
            failure: None,
        }
    }

    pub(super) fn inspected(
        fingerprint: ContentDigest,
        generation: u64,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        memory_count: usize,
        deleted_count: usize,
        compiled_present: bool,
    ) -> Self {
        Self {
            stage: LegacyMemoryCutoverStage::NotStarted,
            source_generation: Some(generation),
            source_fingerprint: Some(fingerprint),
            backup_digest: Some(backup_digest),
            backup_byte_count: Some(backup_byte_count),
            memory_count: memory_count as u32,
            deleted_count: deleted_count as u32,
            compiled_present,
            failure: None,
        }
    }

    pub(super) fn blocked(error: StorageError) -> Self {
        let failure = match error {
            StorageError::InvalidMemory | StorageError::InvalidLegacyRecord { .. } => {
                LegacyMemoryCutoverFailureCode::InvalidSource
            }
            StorageError::RevisionConflict | StorageError::CutoverNotAuthoritative => {
                LegacyMemoryCutoverFailureCode::ConflictingCoreState
            }
            _ => LegacyMemoryCutoverFailureCode::StorageUnavailable,
        };
        Self {
            stage: LegacyMemoryCutoverStage::Blocked,
            source_generation: None,
            source_fingerprint: None,
            backup_digest: None,
            backup_byte_count: None,
            memory_count: 0,
            deleted_count: 0,
            compiled_present: false,
            failure: Some(failure),
        }
    }
}
