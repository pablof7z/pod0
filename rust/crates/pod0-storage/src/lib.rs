#![forbid(unsafe_code)]

mod backup;
mod chapter_import;
mod chapter_import_commit;
mod chapter_import_discard;
mod chapter_import_identity_verification;
mod chapter_import_model;
mod chapter_import_store_read;
mod chapter_import_store_rows;
mod chapter_import_store_write;
mod chapter_import_verification;
mod chapter_legacy_backup;
mod chapter_rollback_database;
mod chapter_rollback_export;
mod chapter_rollback_format;
mod chapter_rollback_manifest;
mod chapter_rollback_verify;
mod chapter_store_codec;
mod chapter_store_read_artifact;
mod chapter_store_write_artifact;
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
mod legacy_chapter_artifact_source;
mod legacy_chapter_db;
mod legacy_chapter_db_artifacts;
mod legacy_chapter_db_schema;
mod legacy_chapter_episode;
mod legacy_chapter_files;
mod legacy_chapter_format;
mod legacy_chapter_source;
mod legacy_chapter_transform;
mod legacy_clip_format;
mod legacy_clip_source;
mod legacy_episode;
mod legacy_format;
mod legacy_note_format;
mod legacy_note_source;
mod legacy_source;
mod legacy_transcript_db;
mod legacy_transcript_db_schema;
mod legacy_transcript_format;
mod legacy_transcript_source;
mod legacy_transcript_transform;
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
mod schema_chapters;
mod schema_clips;
mod schema_evidence;
mod schema_introspection;
mod schema_library;
mod schema_notes;
mod schema_transcripts;
mod transcript_authority;
mod transcript_backup_atomic;
mod transcript_import;
mod transcript_import_commit;
mod transcript_import_digest;
mod transcript_import_discard;
mod transcript_import_model;
mod transcript_import_store_read;
mod transcript_import_store_write;
mod transcript_import_verification;
mod transcript_legacy_backup;
mod transcript_rollback_export;
mod transcript_rollback_format;
mod transcript_store;
mod transcript_store_codec;
mod transcript_store_model;
mod transcript_store_read;
mod transcript_store_read_artifact;
mod transcript_store_read_rows;
mod transcript_store_write;
mod transcript_store_write_artifact;
mod transcript_store_write_rows;

pub use backup::{restore_backup_to_new_store, verify_backup};
pub use chapter_import::{ChapterImportClock, ChapterImporter};
pub use chapter_import_model::{
    ChapterBackupEvidence, ChapterImportPlan, ChapterImportReport, ChapterImportState,
    ChapterImportVerification, ChapterRollbackExportReport, LegacyChapterSourceKind,
};
pub(crate) use chapter_import_model::{
    ChapterEvidenceKind, ChapterEvidenceValidation, InspectedChapterEvidence,
    InspectedChapterSource, LegacyAdSpanIdentity, LegacyChapterIdentity, StoredChapterEvidence,
};
pub use chapter_import_store_read::{read_active_chapter_import, read_chapter_import};
pub use chapter_rollback_export::{
    CHAPTER_ROLLBACK_FORMAT_VERSION, export_chapter_rollback_bundle,
};
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
pub use legacy_chapter_source::inspect_legacy_chapter_source;
pub use legacy_clip_source::inspect_legacy_clip_source;
pub use legacy_note_source::inspect_legacy_note_source;
pub use legacy_source::inspect_legacy_listening_source;
pub use legacy_transcript_source::inspect_legacy_transcript_source;
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
pub use transcript_import::{TranscriptImportClock, TranscriptImporter};
pub use transcript_import_model::{
    LegacyTranscriptSourceKind, TranscriptBackupEvidence, TranscriptImportEntrySummary,
    TranscriptImportPlan, TranscriptImportReport, TranscriptImportState,
    TranscriptImportVerification,
};
pub use transcript_import_store_read::{
    read_active_transcript_import, read_transcript_import, read_transcript_import_entries,
};
pub use transcript_rollback_export::{
    TranscriptRollbackExportReport, export_transcript_rollback_bundle,
};
pub use transcript_store::{TranscriptStore, transcript_store_is_authoritative};
pub use transcript_store_model::{
    MAX_TRANSCRIPT_PROJECTION_ITEMS, StoredTranscriptSegment, StoredTranscriptSpeaker,
    StoredTranscriptWord, TranscriptCommitStorageReceipt, TranscriptPage,
    TranscriptSelectionSummary,
};

#[cfg(test)]
mod chapter_import_evidence_tests;
#[cfg(test)]
mod chapter_import_failure_tests;
#[cfg(test)]
mod chapter_import_recovery_tests;
#[cfg(test)]
mod chapter_import_source_tests;
#[cfg(test)]
mod chapter_import_test_support;
#[cfg(test)]
mod chapter_import_tests;
#[cfg(test)]
mod chapter_rollback_export_tests;
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
mod migration_chapter_tests;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod migration_transcript_history_tests;
#[cfg(test)]
mod note_import_tests;
#[cfg(test)]
mod recovery_test_support;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod transcript_import_empty_tests;
#[cfg(test)]
mod transcript_import_evidence_tests;
#[cfg(test)]
mod transcript_import_failure_tests;
#[cfg(test)]
mod transcript_import_history_tests;
#[cfg(test)]
mod transcript_import_recovery_tests;
#[cfg(test)]
mod transcript_import_supersession_tests;
#[cfg(test)]
mod transcript_import_test_support;
#[cfg(test)]
mod transcript_import_tests;
#[cfg(test)]
mod transcript_rollback_export_tests;
#[cfg(test)]
mod transcript_store_recovery_tests;
#[cfg(test)]
mod transcript_store_test_support;
#[cfg(test)]
mod transcript_store_tests;
