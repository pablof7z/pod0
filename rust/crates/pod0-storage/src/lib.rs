#![forbid(unsafe_code)]

mod backup;
mod evidence_codec;
mod evidence_commands;
mod evidence_model;
mod evidence_store;
mod evidence_store_mutations;
mod evidence_store_read;
mod evidence_store_stage;
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
mod library_store_playback;
mod library_store_playback_apply;
mod library_store_playback_queue;
mod library_store_playback_support;
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
mod schema_evidence;
mod schema_introspection;
mod schema_library;

pub use backup::{restore_backup_to_new_store, verify_backup};
pub use evidence_model::{
    EvidenceGenerationState, EvidenceGenerationSummary, EvidencePruneReceipt,
    EvidenceSelectionReceipt, EvidenceStageReceipt, EvidenceVerificationReceipt,
};
pub use evidence_store::EvidenceStore;
pub use import_model::{
    LegacyBackupEvidence, LegacyImportPlan, LegacySourceKind, ListeningImportReport,
    ListeningImportVerification,
};
pub use legacy_source::inspect_legacy_listening_source;
pub use library_store::{LibraryStore, commit_listening_cutover};
pub use library_store_playback::{
    PlaybackMutation, PlaybackMutationResult, PlaybackQueuePlacement,
};
pub use listening_import::{ListeningImportClock, ListeningImporter};
pub use listening_store::read_listening_import;
pub use migration::{CoreStoreMigrator, MigrationClock};
pub use model::{
    APPLICATION_ID, AccessMode, BackupEvidence, BlockedReason, CURRENT_SCHEMA_VERSION,
    MIN_SUPPORTED_SCHEMA_VERSION, MigrationReport, MigrationState, SchemaStatus, StorageError,
};

#[cfg(test)]
mod evidence_store_recovery_tests;
#[cfg(test)]
mod evidence_store_test_support;
#[cfg(test)]
mod evidence_store_tests;
#[cfg(test)]
mod library_store_playback_tests;
#[cfg(test)]
mod library_store_synthetic_tests;
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
