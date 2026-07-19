use std::path::Path;

use pod0_storage::{
    NoteBackupEvidence, NoteImportClock, NoteImportPlan, NoteImportReport, NoteImporter,
    StorageError,
};

use crate::{CommandId, LegacyListeningSourceKind, NoteRecord, StateRevision};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyNoteImportPlan {
    pub source_kind: LegacyListeningSourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub note_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyNoteBackupEvidence {
    pub source_kind: LegacyListeningSourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub byte_count: u64,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyNoteImportReport {
    pub import_id: CommandId,
    pub plan: LegacyNoteImportPlan,
    pub target_revision: StateRevision,
    pub backup: LegacyNoteBackupEvidence,
    pub staged: bool,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyNoteImportVerification {
    pub report: LegacyNoteImportReport,
    pub collection_revision: StateRevision,
    pub notes: Vec<NoteRecord>,
}

#[derive(Debug, uniffi::Error)]
pub enum LegacyNoteMigrationError {
    SourceChanged,
    SourceInvalid,
    BackupConflict,
    ImportConflict,
    ImportNotFound,
    TargetBlocked,
    Interrupted,
    StorageUnavailable,
}

impl std::fmt::Display for LegacyNoteMigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::SourceChanged => "legacy note source changed",
            Self::SourceInvalid => "legacy note source is invalid",
            Self::BackupConflict => "legacy note backup conflicts with the source",
            Self::ImportConflict => "staged note import conflicts with existing state",
            Self::ImportNotFound => "staged note import was not found",
            Self::TargetBlocked => "shared note store cannot be migrated safely",
            Self::Interrupted => "note import was interrupted before commit",
            Self::StorageUnavailable => "note storage is unavailable",
        })
    }
}

impl std::error::Error for LegacyNoteMigrationError {}

#[uniffi::export]
pub fn inspect_legacy_note_source(
    source_path: String,
) -> Result<LegacyNoteImportPlan, LegacyNoteMigrationError> {
    pod0_storage::inspect_legacy_note_source(Path::new(&source_path))
        .map(Into::into)
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
#[uniffi::export]
pub fn stage_legacy_note_import(
    source_path: String,
    source_backup_path: String,
    target_path: String,
    target_schema_backup_path: String,
    expected_plan: LegacyNoteImportPlan,
    import_id: CommandId,
    target_store_id: CommandId,
    observed_at_milliseconds: i64,
) -> Result<LegacyNoteImportReport, LegacyNoteMigrationError> {
    NoteImporter::new(FixedClock(observed_at_milliseconds))
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
pub fn read_staged_legacy_note_import(
    target_path: String,
    import_id: CommandId,
) -> Result<LegacyNoteImportVerification, LegacyNoteMigrationError> {
    pod0_storage::read_note_import(Path::new(&target_path), import_id)
        .map(|value| LegacyNoteImportVerification {
            report: value.report.into(),
            collection_revision: value.snapshot.revision,
            notes: value.snapshot.notes,
        })
        .map_err(Into::into)
}

#[uniffi::export]
pub fn commit_staged_legacy_note_import(
    target_path: String,
    observed_at_milliseconds: i64,
) -> Result<bool, LegacyNoteMigrationError> {
    pod0_storage::commit_note_cutover(Path::new(&target_path), observed_at_milliseconds)
        .map_err(Into::into)
}

struct FixedClock(i64);

impl NoteImportClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        self.0
    }
}

impl From<NoteImportPlan> for LegacyNoteImportPlan {
    fn from(value: NoteImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            note_count: value.note_count,
        }
    }
}

impl From<LegacyNoteImportPlan> for NoteImportPlan {
    fn from(value: LegacyNoteImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            note_count: value.note_count,
        }
    }
}

impl From<NoteBackupEvidence> for LegacyNoteBackupEvidence {
    fn from(value: NoteBackupEvidence) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            byte_count: value.byte_count,
            reused_existing: value.reused_existing,
        }
    }
}

impl From<NoteImportReport> for LegacyNoteImportReport {
    fn from(value: NoteImportReport) -> Self {
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

impl From<StorageError> for LegacyNoteMigrationError {
    fn from(value: StorageError) -> Self {
        match value {
            StorageError::SourceChanged => Self::SourceChanged,
            StorageError::BackupConflict => Self::BackupConflict,
            StorageError::ImportConflict
            | StorageError::CutoverAlreadyAuthoritative
            | StorageError::CommandConflict
            | StorageError::RevisionConflict => Self::ImportConflict,
            StorageError::InvalidNote => Self::SourceInvalid,
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
