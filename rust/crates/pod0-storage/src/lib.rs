#![forbid(unsafe_code)]

mod backup;
mod migration;
mod migration_db;
mod model;
mod schema;

pub use backup::{restore_backup_to_new_store, verify_backup};
pub use migration::{CoreStoreMigrator, MigrationClock};
pub use model::{
    APPLICATION_ID, AccessMode, BackupEvidence, BlockedReason, CURRENT_SCHEMA_VERSION,
    MIN_SUPPORTED_SCHEMA_VERSION, MigrationReport, MigrationState, SchemaStatus, StorageError,
};

#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod recovery_tests;
