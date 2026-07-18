use std::path::Path;

use pod0_storage::{
    LegacyBackupEvidence, LegacyImportPlan, LegacySourceKind, ListeningImportClock,
    ListeningImportReport, ListeningImporter, StorageError,
};

use crate::{CommandId, ListeningDomainSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyListeningSourceKind {
    SwiftSqlite,
    LegacyJson,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyListeningImportPlan {
    pub source_kind: LegacyListeningSourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub podcast_count: u32,
    pub subscription_count: u32,
    pub episode_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyListeningBackupEvidence {
    pub source_kind: LegacyListeningSourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub byte_count: u64,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyListeningImportReport {
    pub import_id: CommandId,
    pub plan: LegacyListeningImportPlan,
    pub target_revision: u64,
    pub backup: LegacyListeningBackupEvidence,
    pub staged: bool,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyListeningImportVerification {
    pub report: LegacyListeningImportReport,
    pub snapshot: ListeningDomainSnapshot,
}

#[derive(Debug, uniffi::Error)]
pub enum LegacyListeningMigrationError {
    SourceChanged,
    SourceInvalid,
    BackupConflict,
    ImportConflict,
    ImportNotFound,
    TargetBlocked,
    Interrupted,
    StorageUnavailable,
}

impl std::fmt::Display for LegacyListeningMigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::SourceChanged => "legacy listening source changed",
            Self::SourceInvalid => "legacy listening source is invalid",
            Self::BackupConflict => "legacy listening backup conflicts with the source",
            Self::ImportConflict => "staged listening import conflicts with existing state",
            Self::ImportNotFound => "staged listening import was not found",
            Self::TargetBlocked => "shared listening store cannot be migrated safely",
            Self::Interrupted => "listening import was interrupted before commit",
            Self::StorageUnavailable => "listening storage is unavailable",
        })
    }
}

impl std::error::Error for LegacyListeningMigrationError {}

#[uniffi::export]
pub fn inspect_legacy_listening_source(
    source_path: String,
) -> Result<LegacyListeningImportPlan, LegacyListeningMigrationError> {
    pod0_storage::inspect_legacy_listening_source(Path::new(&source_path))
        .map(Into::into)
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
#[uniffi::export]
pub fn stage_legacy_listening_import(
    source_path: String,
    source_backup_path: String,
    target_path: String,
    target_schema_backup_path: String,
    expected_plan: LegacyListeningImportPlan,
    import_id: CommandId,
    target_store_id: CommandId,
    observed_at_milliseconds: i64,
) -> Result<LegacyListeningImportReport, LegacyListeningMigrationError> {
    ListeningImporter::new(FixedClock(observed_at_milliseconds))
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
pub fn read_staged_legacy_listening_import(
    target_path: String,
    import_id: CommandId,
) -> Result<LegacyListeningImportVerification, LegacyListeningMigrationError> {
    pod0_storage::read_listening_import(Path::new(&target_path), import_id)
        .map(|value| LegacyListeningImportVerification {
            report: value.report.into(),
            snapshot: value.snapshot,
        })
        .map_err(Into::into)
}

struct FixedClock(i64);
impl ListeningImportClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        self.0
    }
}

impl From<LegacyImportPlan> for LegacyListeningImportPlan {
    fn from(value: LegacyImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            podcast_count: value.podcast_count,
            subscription_count: value.subscription_count,
            episode_count: value.episode_count,
        }
    }
}
impl From<LegacyListeningImportPlan> for LegacyImportPlan {
    fn from(value: LegacyListeningImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            podcast_count: value.podcast_count,
            subscription_count: value.subscription_count,
            episode_count: value.episode_count,
        }
    }
}
impl From<LegacySourceKind> for LegacyListeningSourceKind {
    fn from(value: LegacySourceKind) -> Self {
        match value {
            LegacySourceKind::SwiftSqlite => Self::SwiftSqlite,
            LegacySourceKind::LegacyJson => Self::LegacyJson,
        }
    }
}
impl From<LegacyListeningSourceKind> for LegacySourceKind {
    fn from(value: LegacyListeningSourceKind) -> Self {
        match value {
            LegacyListeningSourceKind::SwiftSqlite => Self::SwiftSqlite,
            LegacyListeningSourceKind::LegacyJson => Self::LegacyJson,
        }
    }
}
impl From<LegacyBackupEvidence> for LegacyListeningBackupEvidence {
    fn from(value: LegacyBackupEvidence) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_hash: value.source_hash,
            source_generation: value.source_generation,
            byte_count: value.byte_count,
            reused_existing: value.reused_existing,
        }
    }
}
impl From<ListeningImportReport> for LegacyListeningImportReport {
    fn from(value: ListeningImportReport) -> Self {
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
impl From<StorageError> for LegacyListeningMigrationError {
    fn from(value: StorageError) -> Self {
        match value {
            StorageError::SourceChanged => Self::SourceChanged,
            StorageError::BackupConflict => Self::BackupConflict,
            StorageError::ImportConflict | StorageError::CutoverAlreadyAuthoritative => {
                Self::ImportConflict
            }
            StorageError::ImportNotFound => Self::ImportNotFound,
            StorageError::Interrupted => Self::Interrupted,
            StorageError::UnsupportedLegacySource
            | StorageError::InvalidLegacyRecord { .. }
            | StorageError::ImportLimitExceeded { .. } => Self::SourceInvalid,
            StorageError::UnsupportedTarget { .. }
            | StorageError::DowngradeForbidden { .. }
            | StorageError::NewerSchema { .. }
            | StorageError::ForeignDatabase
            | StorageError::CorruptSchema { .. }
            | StorageError::FailedMigration { .. } => Self::TargetBlocked,
            StorageError::Io { .. } | StorageError::Sqlite { .. } => Self::StorageUnavailable,
        }
    }
}
