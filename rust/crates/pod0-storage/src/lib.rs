#![forbid(unsafe_code)]
mod agent_store;
mod agent_store_codec;
mod agent_store_history;
mod agent_store_model;
mod agent_store_recovery;
#[cfg(test)]
mod agent_store_tests;
mod backup;
mod chapter_authority;
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
mod chapter_store_model;
mod chapter_store_read_artifact;
mod chapter_store_read_selection;
mod chapter_store_receipt;
mod chapter_store_write_artifact;
mod chapter_workflow_combined;
mod chapter_workflow_model;
mod chapter_workflow_store;
mod chapter_workflow_store_adopt;
mod chapter_workflow_store_complete;
mod chapter_workflow_store_read;
mod chapter_workflow_store_support;
mod chapter_workflow_store_write;
mod clip_import;
mod clip_import_model;
mod clip_import_store;
mod clip_import_store_support;
mod clip_legacy_backup;
mod clip_store_codec;
mod clip_store_model;
mod clip_store_read;
mod download_store;
mod download_store_artifact;
mod download_store_artifact_file;
mod download_store_artifact_recovery;
mod download_store_cancel;
mod download_store_cutover;
mod download_store_cutover_discard;
mod download_store_cutover_entry;
mod download_store_cutover_model;
mod download_store_model;
mod download_store_observations;
mod download_store_read;
mod download_store_request;
mod download_store_retry;
mod download_store_write;
mod evidence_codec;
mod evidence_commands;
mod evidence_model;
mod evidence_store;
mod evidence_store_mutations;
mod evidence_store_read;
mod evidence_store_stage;
mod exports;
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
mod library_store_chapters;
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
mod model_chapter_workflow;
mod note_import;
mod note_import_model;
mod note_import_store;
mod note_import_store_support;
mod note_legacy_backup;
mod note_store_codec;
mod note_store_model;
mod note_store_read;
mod recall_configuration_store;
mod retained_orphan_parent;
mod scheduled_agent_cutover;
mod scheduled_agent_cutover_model;
mod scheduled_agent_cutover_read;
mod scheduled_agent_cutover_stage;
mod scheduled_agent_cutover_validation;
mod scheduled_agent_store;
mod scheduled_agent_store_actions;
mod scheduled_agent_store_codec;
mod scheduled_agent_store_completion;
mod scheduled_agent_store_model;
mod scheduled_agent_store_observation_fingerprint;
mod scheduled_agent_store_observations;
mod scheduled_agent_store_read;
mod scheduled_agent_store_reconcile;
mod scheduled_agent_store_recovery;
mod scheduled_agent_store_tasks;
mod schema;
mod schema_agent;
mod schema_chapter_workflows;
mod schema_chapters;
mod schema_clips;
mod schema_download_workflows;
mod schema_evidence;
mod schema_introspection;
mod schema_library;
mod schema_migrations;
mod schema_model_chapter_workflows;
mod schema_notes;
mod schema_scheduled_agent;
mod schema_transcript_workflows;
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
mod transcript_workflow;
pub(crate) use chapter_import_model::{
    ChapterEvidenceKind, ChapterEvidenceValidation, InspectedChapterEvidence,
    InspectedChapterSource, LegacyAdSpanIdentity, LegacyChapterIdentity, StoredChapterEvidence,
};
pub(crate) use clip_import_model::InspectedClipSource;
pub use exports::*;
pub(crate) use note_import_model::InspectedNoteSource;
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
mod chapter_store_read_tests;
#[cfg(test)]
mod chapter_workflow_test_support;
#[cfg(test)]
mod chapter_workflow_tests;
#[cfg(test)]
mod clip_cutover_restart_tests;
#[cfg(test)]
mod clip_import_failure_tests;
#[cfg(test)]
mod clip_import_orphan_tests;
#[cfg(test)]
mod clip_import_tests;
#[cfg(test)]
mod download_store_artifact_tests;
#[cfg(test)]
mod download_store_cutover_recovery_tests;
#[cfg(test)]
mod download_store_cutover_tests;
#[cfg(test)]
mod download_store_lifecycle_tests;
#[cfg(test)]
mod download_store_test_support;
#[cfg(test)]
mod download_store_tests;
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
mod migration_chapter_workflow_tests;
#[cfg(test)]
mod migration_scheduled_agent_tests;
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
mod scheduled_agent_cutover_tests;
#[cfg(test)]
mod scheduled_agent_store_lifecycle_tests;
#[cfg(test)]
mod scheduled_agent_store_schema_tests;
#[cfg(test)]
mod scheduled_agent_store_test_support;
#[cfg(test)]
mod scheduled_agent_store_tests;
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
