#![forbid(unsafe_code)]

mod backup;
mod clip_import;
mod clip_import_model;
mod clip_import_store;
mod clip_import_store_support;
mod clip_legacy_backup;
mod clip_store_codec;
mod clip_store_model;
mod clip_store_read;
mod evidence_codec;
mod evidence_commands;
mod evidence_model;
mod evidence_store;
mod evidence_store_mutations;
mod evidence_store_read;
mod evidence_store_stage;
mod import_model;
mod legacy_backup;
mod legacy_clip_format;
mod legacy_clip_source;
mod legacy_episode;
mod legacy_format;
mod legacy_note_format;
mod legacy_note_source;
mod legacy_source;
mod legacy_transform;
mod library_feed_codec;
mod library_store;
mod library_store_clip_support;
mod library_store_clips;
mod library_store_commands;
mod library_store_external;
mod library_store_feed;
mod library_store_note_support;
mod library_store_notes;
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
mod note_import;
mod note_import_model;
mod note_import_store;
mod note_import_store_support;
mod note_legacy_backup;
mod note_store_codec;
mod note_store_model;
mod note_store_read;
mod schema;
mod schema_clips;
mod schema_evidence;
mod schema_introspection;
mod schema_library;
mod schema_notes;

pub use backup::{restore_backup_to_new_store, verify_backup};
pub use clip_import::{ClipImportClock, ClipImporter};
pub(crate) use clip_import_model::InspectedClipSource;
pub use clip_import_model::{
    ClipBackupEvidence, ClipImportPlan, ClipImportReport, ClipImportVerification,
};
pub use clip_import_store::{commit_clip_cutover, read_clip_import};
pub use clip_store_model::ClipCollectionSnapshot;
pub use evidence_model::{
    EvidenceGenerationState, EvidenceGenerationSummary, EvidencePruneReceipt,
    EvidenceSelectionReceipt, EvidenceStageReceipt, EvidenceVerificationReceipt,
};
pub use evidence_store::EvidenceStore;
pub use import_model::{
    LegacyBackupEvidence, LegacyImportPlan, LegacySourceKind, ListeningImportReport,
    ListeningImportVerification,
};
pub use legacy_clip_source::inspect_legacy_clip_source;
pub use legacy_note_source::inspect_legacy_note_source;
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
pub use note_import::{NoteImportClock, NoteImporter};
pub(crate) use note_import_model::InspectedNoteSource;
pub use note_import_model::{
    NoteBackupEvidence, NoteImportPlan, NoteImportReport, NoteImportVerification,
};
pub use note_import_store::{commit_note_cutover, read_note_import};
pub use note_store_model::NoteCollectionSnapshot;

#[cfg(test)]
mod clip_cutover_restart_tests;
#[cfg(test)]
mod clip_import_failure_tests;
#[cfg(test)]
mod clip_import_orphan_tests;
#[cfg(test)]
mod clip_import_tests;
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
mod note_import_tests;
#[cfg(test)]
mod recovery_test_support;
#[cfg(test)]
mod recovery_tests;
