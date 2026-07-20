use std::path::Path;

use pod0_storage::ChapterImporter;

use crate::runtime_clock::SystemClock;
use crate::{CommandId, ContentDigest, StateRevision};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyChapterSourceKind {
    ArtifactSqliteV0,
    ArtifactSqliteV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyChapterImportState {
    Staged,
    Verified,
    Imported,
    Corrupt,
    Discarded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyChapterMigrationStage {
    NotStarted,
    Inspected,
    Staged,
    Verified,
    Imported,
    Discarded,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyChapterMigrationFailureCode {
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

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyChapterImportPlan {
    pub source_kind: LegacyChapterSourceKind,
    pub source_generation: u64,
    pub source_file_identity: ContentDigest,
    pub source_database_byte_count: u64,
    pub source_database_digest: ContentDigest,
    pub source_selection_digest: ContentDigest,
    pub evidence_count: u32,
    pub canonical_artifact_count: u32,
    pub selected_count: u32,
    pub blocked_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyChapterBackupEvidence {
    pub database_digest: ContentDigest,
    pub database_byte_count: u64,
    pub file_count: u32,
    pub file_byte_count: u64,
    pub reused_database: bool,
    pub reused_files: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyChapterImportReport {
    pub import_id: CommandId,
    pub plan: LegacyChapterImportPlan,
    pub target_revision: StateRevision,
    pub backup: LegacyChapterBackupEvidence,
    pub state: LegacyChapterImportState,
    pub diagnostic_code: Option<String>,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyChapterImportVerification {
    pub report: LegacyChapterImportReport,
    pub verified_evidence_count: u32,
    pub verified_artifact_count: u32,
    pub verified_chapter_count: u64,
    pub verified_ad_span_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyChapterRollbackExportReport {
    pub bundle_path: String,
    pub format_version: u32,
    pub core_schema_version: u32,
    pub source_generation: u64,
    pub evidence_count: u32,
    pub artifact_count: u32,
    pub bundle_digest: ContentDigest,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyChapterMigrationFailure {
    pub code: LegacyChapterMigrationFailureCode,
    pub diagnostic_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyChapterMigrationProjection {
    pub stage: LegacyChapterMigrationStage,
    pub plan: Option<LegacyChapterImportPlan>,
    pub report: Option<LegacyChapterImportReport>,
    pub verification: Option<LegacyChapterImportVerification>,
    pub rollback_export: Option<LegacyChapterRollbackExportReport>,
    pub failure: Option<LegacyChapterMigrationFailure>,
}

#[uniffi::export]
pub fn inspect_legacy_chapter_migration(
    source_database_path: String,
    artifact_root_path: String,
) -> LegacyChapterMigrationProjection {
    match pod0_storage::inspect_legacy_chapter_source(
        Path::new(&source_database_path),
        Path::new(&artifact_root_path),
    ) {
        Ok(plan) => LegacyChapterMigrationProjection::inspected(plan.into()),
        Err(error) => LegacyChapterMigrationProjection::blocked(error),
    }
}

#[uniffi::export]
pub fn read_active_legacy_chapter_migration(
    target_path: String,
) -> LegacyChapterMigrationProjection {
    match pod0_storage::read_active_chapter_import(Path::new(&target_path)) {
        Ok(Some(report)) => LegacyChapterMigrationProjection::from_report(report.into()),
        Ok(None) => LegacyChapterMigrationProjection::not_started(),
        Err(error) => LegacyChapterMigrationProjection::blocked(error),
    }
}

#[uniffi::export]
pub fn shared_chapter_store_is_authoritative(target_path: String) -> bool {
    pod0_storage::chapter_store_is_authoritative(Path::new(&target_path)).unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
#[uniffi::export]
pub fn stage_legacy_chapter_import(
    source_database_path: String,
    artifact_root_path: String,
    legacy_backup_root_path: String,
    target_path: String,
    target_schema_backup_path: String,
    expected_plan: LegacyChapterImportPlan,
    import_id: CommandId,
    target_store_id: CommandId,
) -> LegacyChapterMigrationProjection {
    let importer = ChapterImporter::new(SystemClock);
    match importer.stage(
        Path::new(&source_database_path),
        Path::new(&artifact_root_path),
        Path::new(&legacy_backup_root_path),
        Path::new(&target_path),
        Path::new(&target_schema_backup_path),
        &expected_plan.into(),
        import_id,
        target_store_id,
    ) {
        Ok(report) => LegacyChapterMigrationProjection::from_report(report.into()),
        Err(error) => LegacyChapterMigrationProjection::blocked_with_report(
            Path::new(&target_path),
            import_id,
            error,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
#[uniffi::export]
pub fn verify_staged_legacy_chapter_import(
    source_database_path: String,
    artifact_root_path: String,
    legacy_backup_root_path: String,
    target_path: String,
    import_id: CommandId,
) -> LegacyChapterMigrationProjection {
    let importer = ChapterImporter::new(SystemClock);
    match importer.verify(
        Path::new(&source_database_path),
        Path::new(&artifact_root_path),
        Path::new(&legacy_backup_root_path),
        Path::new(&target_path),
        import_id,
    ) {
        Ok(verification) => LegacyChapterMigrationProjection::verified(verification.into()),
        Err(error) => LegacyChapterMigrationProjection::blocked_with_report(
            Path::new(&target_path),
            import_id,
            error,
        ),
    }
}

#[uniffi::export]
pub fn commit_staged_legacy_chapter_import(
    source_database_path: String,
    artifact_root_path: String,
    target_path: String,
    import_id: CommandId,
) -> LegacyChapterMigrationProjection {
    let importer = ChapterImporter::new(SystemClock);
    match importer.commit(
        Path::new(&source_database_path),
        Path::new(&artifact_root_path),
        Path::new(&target_path),
        import_id,
    ) {
        Ok(report) => LegacyChapterMigrationProjection::from_report(report.into()),
        Err(error) => LegacyChapterMigrationProjection::blocked_with_report(
            Path::new(&target_path),
            import_id,
            error,
        ),
    }
}

#[uniffi::export]
pub fn discard_staged_legacy_chapter_import(
    target_path: String,
    import_id: CommandId,
) -> LegacyChapterMigrationProjection {
    let importer = ChapterImporter::new(SystemClock);
    match importer.discard(Path::new(&target_path), import_id) {
        Ok(report) => LegacyChapterMigrationProjection::from_report(report.into()),
        Err(error) => LegacyChapterMigrationProjection::blocked_with_report(
            Path::new(&target_path),
            import_id,
            error,
        ),
    }
}

#[uniffi::export]
pub fn export_legacy_chapter_rollback(
    target_path: String,
    legacy_backup_root_path: String,
    export_root_path: String,
) -> LegacyChapterMigrationProjection {
    match pod0_storage::export_chapter_rollback_bundle(
        Path::new(&target_path),
        Path::new(&legacy_backup_root_path),
        Path::new(&export_root_path),
    ) {
        Ok(report) => LegacyChapterMigrationProjection::rollback(report),
        Err(error) => LegacyChapterMigrationProjection::blocked(error),
    }
}
