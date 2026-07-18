#![forbid(unsafe_code)]

mod backup;
mod import_model;
mod legacy_backup;
mod legacy_episode;
mod legacy_format;
mod legacy_source;
mod legacy_transform;
mod library_feed_codec;
mod library_store;
mod library_store_commands;
mod library_store_external;
mod library_store_feed;
mod listening_db_codec;
mod listening_import;
mod listening_store;
mod listening_store_read;
mod listening_store_read_episode;
mod listening_store_write;
mod listening_store_write_entities;
mod migration;
mod migration_db;
mod model;
mod schema;
mod schema_introspection;
mod schema_library;

pub use backup::{restore_backup_to_new_store, verify_backup};
pub use import_model::{
    LegacyBackupEvidence, LegacyImportPlan, LegacySourceKind, ListeningImportReport,
    ListeningImportVerification,
};
pub use legacy_source::inspect_legacy_listening_source;
pub use library_store::{LibraryStore, commit_listening_cutover};
pub use listening_import::{ListeningImportClock, ListeningImporter};
pub use listening_store::read_listening_import;
pub use migration::{CoreStoreMigrator, MigrationClock};
pub use model::{
    APPLICATION_ID, AccessMode, BackupEvidence, BlockedReason, CURRENT_SCHEMA_VERSION,
    MIN_SUPPORTED_SCHEMA_VERSION, MigrationReport, MigrationState, SchemaStatus, StorageError,
};

#[cfg(test)]
mod library_store_tests;
#[cfg(test)]
mod listening_import_failure_tests;
#[cfg(test)]
mod listening_import_test_support;
#[cfg(test)]
mod listening_import_tests;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod recovery_test_support;
#[cfg(test)]
mod recovery_tests;
