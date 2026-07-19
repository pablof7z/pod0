use std::path::Path;

use pod0_storage::{
    ClipBackupEvidence, ClipImportClock, ClipImportPlan, ClipImportReport, ClipImporter,
    StorageError,
};

use crate::{ClipRecord, CommandId, LegacyListeningSourceKind, StateRevision};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyClipImportPlan {
    pub source_kind: LegacyListeningSourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub clip_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyClipBackupEvidence {
    pub source_kind: LegacyListeningSourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub byte_count: u64,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyClipImportReport {
    pub import_id: CommandId,
    pub plan: LegacyClipImportPlan,
    pub target_revision: StateRevision,
    pub backup: LegacyClipBackupEvidence,
    pub staged: bool,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyClipImportVerification {
    pub report: LegacyClipImportReport,
    pub collection_revision: StateRevision,
    pub clips: Vec<ClipRecord>,
}

#[derive(Debug, uniffi::Error)]
pub enum LegacyClipMigrationError {
    SourceChanged,
    SourceInvalid,
    BackupConflict,
    ImportConflict,
    ImportNotFound,
    TargetBlocked,
    Interrupted,
    StorageUnavailable,
}

impl std::fmt::Display for LegacyClipMigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::SourceChanged => "legacy clip source changed",
            Self::SourceInvalid => "legacy clip source is invalid",
            Self::BackupConflict => "legacy clip backup conflicts with the source",
            Self::ImportConflict => "staged clip import conflicts with existing state",
            Self::ImportNotFound => "staged clip import was not found",
            Self::TargetBlocked => "shared clip store cannot be migrated safely",
            Self::Interrupted => "clip import was interrupted before commit",
            Self::StorageUnavailable => "clip storage is unavailable",
        })
    }
}

impl std::error::Error for LegacyClipMigrationError {}

#[uniffi::export]
pub fn inspect_legacy_clip_source(
    source_path: String,
) -> Result<LegacyClipImportPlan, LegacyClipMigrationError> {
    pod0_storage::inspect_legacy_clip_source(Path::new(&source_path))
        .map(Into::into)
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
#[uniffi::export]
pub fn stage_legacy_clip_import(
    source_path: String,
    source_backup_path: String,
    target_path: String,
    target_schema_backup_path: String,
    expected_plan: LegacyClipImportPlan,
    import_id: CommandId,
    target_store_id: CommandId,
    observed_at_milliseconds: i64,
) -> Result<LegacyClipImportReport, LegacyClipMigrationError> {
    ClipImporter::new(FixedClock(observed_at_milliseconds))
        .stage(
            Path::new(&source_path),
            Path::new(&source_backup_path),
            Path::new(&target_path),
            Path::new(&target_schema_backup_path),
            &expected_plan.into(),
            import_id,
            target_store_id,
        )
        .map(Into::into)
        .map_err(Into::into)
}

#[uniffi::export]
pub fn read_staged_legacy_clip_import(
    target_path: String,
    import_id: CommandId,
) -> Result<LegacyClipImportVerification, LegacyClipMigrationError> {
    pod0_storage::read_clip_import(Path::new(&target_path), import_id)
        .map(|value| LegacyClipImportVerification {
            report: value.report.into(),
            collection_revision: value.snapshot.revision,
            clips: value.snapshot.clips,
        })
        .map_err(Into::into)
}

#[uniffi::export]
pub fn commit_staged_legacy_clip_import(
    source_path: String,
    target_path: String,
    observed_at_milliseconds: i64,
) -> Result<bool, LegacyClipMigrationError> {
    pod0_storage::commit_clip_cutover(
        Path::new(&source_path),
        Path::new(&target_path),
        observed_at_milliseconds,
    )
    .map_err(Into::into)
}

struct FixedClock(i64);

impl ClipImportClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        self.0
    }
}

impl From<ClipImportPlan> for LegacyClipImportPlan {
    fn from(value: ClipImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            clip_count: value.clip_count,
        }
    }
}

impl From<LegacyClipImportPlan> for ClipImportPlan {
    fn from(value: LegacyClipImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            clip_count: value.clip_count,
        }
    }
}

impl From<ClipBackupEvidence> for LegacyClipBackupEvidence {
    fn from(value: ClipBackupEvidence) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            byte_count: value.byte_count,
            reused_existing: value.reused_existing,
        }
    }
}

impl From<ClipImportReport> for LegacyClipImportReport {
    fn from(value: ClipImportReport) -> Self {
        Self {
            import_id: value.import_id,
            plan: value.plan.into(),
            target_revision: value.target_revision,
            backup: value.backup.into(),
            staged: value.staged,
            reused_existing: value.reused_existing,
        }
    }
}

impl From<StorageError> for LegacyClipMigrationError {
    fn from(value: StorageError) -> Self {
        match value {
            StorageError::SourceChanged => Self::SourceChanged,
            StorageError::BackupConflict => Self::BackupConflict,
            StorageError::ImportConflict
            | StorageError::CutoverAlreadyAuthoritative
            | StorageError::CommandConflict
            | StorageError::RevisionConflict => Self::ImportConflict,
            StorageError::InvalidClip => Self::SourceInvalid,
            StorageError::ImportNotFound | StorageError::EntityNotFound => Self::ImportNotFound,
            StorageError::Interrupted => Self::Interrupted,
            StorageError::UnsupportedLegacySource
            | StorageError::InvalidLegacyRecord { .. }
            | StorageError::ImportLimitExceeded { .. } => Self::SourceInvalid,
            StorageError::Io { .. } | StorageError::Sqlite { .. } => Self::StorageUnavailable,
            _ => Self::TargetBlocked,
        }
    }
}
