use std::fmt;
use std::path::PathBuf;

use pod0_domain::CommandId;

pub const APPLICATION_ID: i64 = 0x504F_4430;
pub const MIN_SUPPORTED_SCHEMA_VERSION: u32 = 0;
pub const CURRENT_SCHEMA_VERSION: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessMode {
    ReadWrite,
    MigrationOnly,
    ReadOnlyRecovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockedReason {
    Corrupt,
    ForeignDatabase,
    NewerSchema,
    FailedMigration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationState {
    Fresh,
    Required {
        target_version: u32,
    },
    InProgress {
        from_version: u32,
        target_version: u32,
    },
    Ready,
    Blocked(BlockedReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaStatus {
    pub stored_version: Option<u32>,
    pub supported_min: u32,
    pub supported_max: u32,
    pub access_mode: AccessMode,
    pub migration_state: MigrationState,
}

impl SchemaStatus {
    pub(crate) const fn blocked(stored_version: Option<u32>, reason: BlockedReason) -> Self {
        Self {
            stored_version,
            supported_min: MIN_SUPPORTED_SCHEMA_VERSION,
            supported_max: CURRENT_SCHEMA_VERSION,
            access_mode: AccessMode::ReadOnlyRecovery,
            migration_state: MigrationState::Blocked(reason),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupEvidence {
    pub path: PathBuf,
    pub store_id: CommandId,
    pub schema_version: u32,
    pub byte_count: u64,
    pub page_count: u64,
    pub integrity_check: String,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationReport {
    pub migration_id: CommandId,
    pub from_version: u32,
    pub to_version: u32,
    pub applied_versions: Vec<u32>,
    pub resumed_from_journal: bool,
    pub backup: Option<BackupEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    Io {
        operation: &'static str,
    },
    Sqlite {
        operation: &'static str,
    },
    UnsupportedTarget {
        requested: u32,
        supported: u32,
    },
    DowngradeForbidden {
        stored: u32,
        requested: u32,
    },
    NewerSchema {
        stored: u32,
        supported: u32,
    },
    ForeignDatabase,
    CorruptSchema {
        detail: &'static str,
    },
    FailedMigration {
        from: u32,
        to: u32,
    },
    BackupConflict,
    UnsupportedLegacySource,
    InvalidLegacyRecord {
        entity: &'static str,
        index: u32,
        detail: &'static str,
    },
    ImportLimitExceeded {
        entity: &'static str,
    },
    SourceChanged,
    ImportConflict,
    ImportNotFound,
    CutoverAlreadyAuthoritative,
    CutoverNotAuthoritative,
    CommandConflict,
    EntityNotFound,
    Interrupted,
}

impl StorageError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "storage_io",
            Self::Sqlite { .. } => "storage_sqlite",
            Self::UnsupportedTarget { .. } => "unsupported_schema_target",
            Self::DowngradeForbidden { .. } => "schema_downgrade_forbidden",
            Self::NewerSchema { .. } => "newer_schema",
            Self::ForeignDatabase => "foreign_database",
            Self::CorruptSchema { .. } => "corrupt_schema",
            Self::FailedMigration { .. } => "failed_migration",
            Self::BackupConflict => "backup_conflict",
            Self::UnsupportedLegacySource => "unsupported_legacy_source",
            Self::InvalidLegacyRecord { .. } => "invalid_legacy_record",
            Self::ImportLimitExceeded { .. } => "import_limit_exceeded",
            Self::SourceChanged => "legacy_source_changed",
            Self::ImportConflict => "listening_import_conflict",
            Self::ImportNotFound => "listening_import_not_found",
            Self::CutoverAlreadyAuthoritative => "listening_already_authoritative",
            Self::CutoverNotAuthoritative => "listening_not_authoritative",
            Self::CommandConflict => "library_command_conflict",
            Self::EntityNotFound => "library_entity_not_found",
            Self::Interrupted => "migration_interrupted",
        }
    }

    pub(crate) fn sqlite(operation: &'static str, _: rusqlite::Error) -> Self {
        Self::Sqlite { operation }
    }

    pub(crate) fn io(operation: &'static str, _: std::io::Error) -> Self {
        Self::Io { operation }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Sqlite {
            operation: "decode listening projection",
        }
    }
}

pub(crate) fn command_id(bytes: &[u8]) -> Result<CommandId, StorageError> {
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "stored ID has invalid length",
    })?;
    Ok(CommandId::from_bytes(bytes))
}
