use std::path::Path;

use pod0_storage::{TranscriptImportClock, TranscriptImporter};

use crate::{CommandId, ContentDigest, StateRevision};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyTranscriptSourceKind {
    ArtifactSqliteV0,
    ArtifactSqliteV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyTranscriptImportState {
    Staged,
    Verified,
    Committed,
    Corrupt,
    Discarded,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptImportPlan {
    pub source_kind: LegacyTranscriptSourceKind,
    pub source_generation: u64,
    pub source_database_digest: ContentDigest,
    pub source_selection_digest: ContentDigest,
    pub artifact_count: u32,
    pub selected_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptBackupEvidence {
    pub database_digest: ContentDigest,
    pub database_byte_count: u64,
    pub artifact_count: u32,
    pub artifact_byte_count: u64,
    pub reused_database: bool,
    pub reused_artifacts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptImportReport {
    pub import_id: CommandId,
    pub plan: LegacyTranscriptImportPlan,
    pub target_revision: StateRevision,
    pub backup: LegacyTranscriptBackupEvidence,
    pub state: LegacyTranscriptImportState,
    pub diagnostic_code: Option<String>,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptImportVerification {
    pub report: LegacyTranscriptImportReport,
    pub verified_artifact_count: u32,
    pub verified_segment_count: u64,
    pub verified_word_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyTranscriptRollbackExportReport {
    pub bundle_path: String,
    pub core_schema_version: u32,
    pub transcript_revision: u64,
    pub artifact_count: u32,
    pub selected_count: u32,
    pub reused_existing: bool,
}

#[derive(Debug, uniffi::Error)]
pub enum LegacyTranscriptMigrationError {
    SourceChanged,
    SourceInvalid,
    BackupConflict,
    ImportConflict,
    ImportNotFound,
    AlreadyAuthoritative,
    TargetBlocked,
    Interrupted,
    StorageUnavailable,
}

impl std::fmt::Display for LegacyTranscriptMigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::SourceChanged => "legacy transcript source changed",
            Self::SourceInvalid => "legacy transcript source is invalid",
            Self::BackupConflict => "legacy transcript backup conflicts with the source",
            Self::ImportConflict => "staged transcript import conflicts with existing state",
            Self::ImportNotFound => "staged transcript import was not found",
            Self::AlreadyAuthoritative => "shared transcript store is already authoritative",
            Self::TargetBlocked => "shared transcript store cannot be migrated safely",
            Self::Interrupted => "transcript import was interrupted before commit",
            Self::StorageUnavailable => "transcript storage is unavailable",
        })
    }
}

impl std::error::Error for LegacyTranscriptMigrationError {}

#[uniffi::export]
pub fn shared_transcript_store_is_authoritative(
    target_path: String,
) -> Result<bool, LegacyTranscriptMigrationError> {
    pod0_storage::transcript_store_is_authoritative(Path::new(&target_path)).map_err(Into::into)
}

#[uniffi::export]
pub fn inspect_legacy_transcript_source(
    source_database_path: String,
    transcript_root_path: String,
) -> Result<LegacyTranscriptImportPlan, LegacyTranscriptMigrationError> {
    pod0_storage::inspect_legacy_transcript_source(
        Path::new(&source_database_path),
        Path::new(&transcript_root_path),
    )
    .map(Into::into)
    .map_err(Into::into)
}

#[uniffi::export]
pub fn read_active_legacy_transcript_import(
    target_path: String,
) -> Result<Option<LegacyTranscriptImportReport>, LegacyTranscriptMigrationError> {
    pod0_storage::read_active_transcript_import(Path::new(&target_path))
        .map(|report| report.map(Into::into))
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
#[uniffi::export]
pub fn stage_legacy_transcript_import(
    source_database_path: String,
    transcript_root_path: String,
    legacy_backup_root_path: String,
    target_path: String,
    target_schema_backup_path: String,
    expected_plan: LegacyTranscriptImportPlan,
    import_id: CommandId,
    target_store_id: CommandId,
    observed_at_milliseconds: i64,
) -> Result<LegacyTranscriptImportReport, LegacyTranscriptMigrationError> {
    TranscriptImporter::new(FixedClock(observed_at_milliseconds))
        .stage(
            Path::new(&source_database_path),
            Path::new(&transcript_root_path),
            Path::new(&legacy_backup_root_path),
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
pub fn verify_staged_legacy_transcript_import(
    target_path: String,
    legacy_backup_root_path: String,
    import_id: CommandId,
    observed_at_milliseconds: i64,
) -> Result<LegacyTranscriptImportVerification, LegacyTranscriptMigrationError> {
    TranscriptImporter::new(FixedClock(observed_at_milliseconds))
        .verify(
            Path::new(&target_path),
            Path::new(&legacy_backup_root_path),
            import_id,
        )
        .map(Into::into)
        .map_err(Into::into)
}

#[uniffi::export]
pub fn commit_staged_legacy_transcript_import(
    source_database_path: String,
    transcript_root_path: String,
    target_path: String,
    import_id: CommandId,
    observed_at_milliseconds: i64,
) -> Result<LegacyTranscriptImportReport, LegacyTranscriptMigrationError> {
    TranscriptImporter::new(FixedClock(observed_at_milliseconds))
        .commit(
            Path::new(&source_database_path),
            Path::new(&transcript_root_path),
            Path::new(&target_path),
            import_id,
        )
        .map(Into::into)
        .map_err(Into::into)
}

#[uniffi::export]
pub fn discard_staged_legacy_transcript_import(
    target_path: String,
    import_id: CommandId,
    observed_at_milliseconds: i64,
) -> Result<LegacyTranscriptImportReport, LegacyTranscriptMigrationError> {
    TranscriptImporter::new(FixedClock(observed_at_milliseconds))
        .discard(Path::new(&target_path), import_id)
        .map(Into::into)
        .map_err(Into::into)
}

#[uniffi::export]
pub fn export_legacy_transcript_rollback(
    target_path: String,
    export_root_path: String,
) -> Result<LegacyTranscriptRollbackExportReport, LegacyTranscriptMigrationError> {
    let report = pod0_storage::export_transcript_rollback_bundle(
        Path::new(&target_path),
        Path::new(&export_root_path),
    )
    .map_err(LegacyTranscriptMigrationError::from)?;
    LegacyTranscriptRollbackExportReport::try_from(report)
}

struct FixedClock(i64);

impl TranscriptImportClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        self.0
    }
}
