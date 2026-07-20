use std::path::Path;

use pod0_storage::{
    ChapterBackupEvidence, ChapterImportPlan, ChapterImportReport, ChapterImportState,
    ChapterImportVerification, ChapterRollbackExportReport, StorageError,
};

use crate::chapter_migration::{
    LegacyChapterBackupEvidence, LegacyChapterImportPlan, LegacyChapterImportReport,
    LegacyChapterImportState, LegacyChapterImportVerification, LegacyChapterMigrationFailure,
    LegacyChapterMigrationFailureCode, LegacyChapterMigrationProjection,
    LegacyChapterMigrationStage, LegacyChapterRollbackExportReport, LegacyChapterSourceKind,
};

impl From<pod0_storage::LegacyChapterSourceKind> for LegacyChapterSourceKind {
    fn from(value: pod0_storage::LegacyChapterSourceKind) -> Self {
        match value {
            pod0_storage::LegacyChapterSourceKind::ArtifactSqliteV0 => Self::ArtifactSqliteV0,
            pod0_storage::LegacyChapterSourceKind::ArtifactSqliteV1 => Self::ArtifactSqliteV1,
        }
    }
}

impl From<LegacyChapterSourceKind> for pod0_storage::LegacyChapterSourceKind {
    fn from(value: LegacyChapterSourceKind) -> Self {
        match value {
            LegacyChapterSourceKind::ArtifactSqliteV0 => Self::ArtifactSqliteV0,
            LegacyChapterSourceKind::ArtifactSqliteV1 => Self::ArtifactSqliteV1,
        }
    }
}

impl From<ChapterImportPlan> for LegacyChapterImportPlan {
    fn from(value: ChapterImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_generation: value.source_generation,
            source_file_identity: value.source_file_identity,
            source_database_byte_count: value.source_database_byte_count,
            source_database_digest: value.source_database_digest,
            source_selection_digest: value.source_selection_digest,
            evidence_count: value.evidence_count,
            canonical_artifact_count: value.canonical_artifact_count,
            selected_count: value.selected_count,
            blocked_count: value.blocked_count,
        }
    }
}

impl From<LegacyChapterImportPlan> for ChapterImportPlan {
    fn from(value: LegacyChapterImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_generation: value.source_generation,
            source_file_identity: value.source_file_identity,
            source_database_byte_count: value.source_database_byte_count,
            source_database_digest: value.source_database_digest,
            source_selection_digest: value.source_selection_digest,
            evidence_count: value.evidence_count,
            canonical_artifact_count: value.canonical_artifact_count,
            selected_count: value.selected_count,
            blocked_count: value.blocked_count,
        }
    }
}

impl From<ChapterBackupEvidence> for LegacyChapterBackupEvidence {
    fn from(value: ChapterBackupEvidence) -> Self {
        Self {
            database_digest: value.database_digest,
            database_byte_count: value.database_byte_count,
            file_count: value.file_count,
            file_byte_count: value.file_byte_count,
            reused_database: value.reused_database,
            reused_files: value.reused_files,
        }
    }
}

impl From<ChapterImportState> for LegacyChapterImportState {
    fn from(value: ChapterImportState) -> Self {
        match value {
            ChapterImportState::Staged => Self::Staged,
            ChapterImportState::Verified => Self::Verified,
            ChapterImportState::Imported => Self::Imported,
            ChapterImportState::Corrupt => Self::Corrupt,
            ChapterImportState::Discarded => Self::Discarded,
        }
    }
}

impl From<ChapterImportReport> for LegacyChapterImportReport {
    fn from(value: ChapterImportReport) -> Self {
        Self {
            import_id: value.import_id,
            plan: value.plan.into(),
            target_revision: value.target_revision,
            backup: value.backup.into(),
            state: value.state.into(),
            diagnostic_code: value.diagnostic_code,
            reused_existing: value.reused_existing,
        }
    }
}

impl From<ChapterImportVerification> for LegacyChapterImportVerification {
    fn from(value: ChapterImportVerification) -> Self {
        Self {
            report: value.report.into(),
            verified_evidence_count: value.verified_evidence_count,
            verified_artifact_count: value.verified_artifact_count,
            verified_chapter_count: value.verified_chapter_count,
            verified_ad_span_count: value.verified_ad_span_count,
        }
    }
}

impl LegacyChapterMigrationProjection {
    pub(crate) const fn not_started() -> Self {
        Self {
            stage: LegacyChapterMigrationStage::NotStarted,
            plan: None,
            report: None,
            verification: None,
            rollback_export: None,
            failure: None,
        }
    }

    pub(crate) fn inspected(plan: LegacyChapterImportPlan) -> Self {
        Self {
            stage: LegacyChapterMigrationStage::Inspected,
            plan: Some(plan),
            ..Self::not_started()
        }
    }

    pub(crate) fn from_report(report: LegacyChapterImportReport) -> Self {
        let stage = report_stage(report.state);
        let plan = Some(report.plan.clone());
        let failure = (report.state == LegacyChapterImportState::Corrupt).then(|| {
            LegacyChapterMigrationFailure {
                code: LegacyChapterMigrationFailureCode::TargetBlocked,
                diagnostic_code: report
                    .diagnostic_code
                    .clone()
                    .unwrap_or_else(|| "chapter_import_corrupt".to_owned()),
            }
        });
        Self {
            stage,
            plan,
            report: Some(report),
            verification: None,
            rollback_export: None,
            failure,
        }
    }

    pub(crate) fn verified(verification: LegacyChapterImportVerification) -> Self {
        Self {
            stage: LegacyChapterMigrationStage::Verified,
            plan: Some(verification.report.plan.clone()),
            report: Some(verification.report.clone()),
            verification: Some(verification),
            rollback_export: None,
            failure: None,
        }
    }

    pub(crate) fn rollback(value: ChapterRollbackExportReport) -> Self {
        let Ok(bundle_path) = value.bundle_path.into_os_string().into_string() else {
            return Self::blocked_diagnostic(
                LegacyChapterMigrationFailureCode::StorageUnavailable,
                "rollback_path_not_utf8",
            );
        };
        Self {
            stage: LegacyChapterMigrationStage::Imported,
            plan: None,
            report: None,
            verification: None,
            rollback_export: Some(LegacyChapterRollbackExportReport {
                bundle_path,
                format_version: value.format_version,
                core_schema_version: value.core_schema_version,
                source_generation: value.source_generation,
                evidence_count: value.evidence_count,
                artifact_count: value.artifact_count,
                bundle_digest: value.bundle_digest,
                reused_existing: value.reused_existing,
            }),
            failure: None,
        }
    }

    pub(crate) fn blocked(error: StorageError) -> Self {
        Self::blocked_diagnostic(failure_code(&error), error.code())
    }

    pub(crate) fn blocked_with_report(
        target_path: &Path,
        import_id: crate::CommandId,
        error: StorageError,
    ) -> Self {
        let report = pod0_storage::read_chapter_import(target_path, import_id)
            .ok()
            .map(Into::into);
        Self {
            stage: LegacyChapterMigrationStage::Blocked,
            plan: report
                .as_ref()
                .map(|report: &LegacyChapterImportReport| report.plan.clone()),
            report,
            verification: None,
            rollback_export: None,
            failure: Some(LegacyChapterMigrationFailure {
                code: failure_code(&error),
                diagnostic_code: error.code().to_owned(),
            }),
        }
    }

    fn blocked_diagnostic(code: LegacyChapterMigrationFailureCode, diagnostic: &str) -> Self {
        Self {
            stage: LegacyChapterMigrationStage::Blocked,
            plan: None,
            report: None,
            verification: None,
            rollback_export: None,
            failure: Some(LegacyChapterMigrationFailure {
                code,
                diagnostic_code: diagnostic.to_owned(),
            }),
        }
    }
}

const fn report_stage(state: LegacyChapterImportState) -> LegacyChapterMigrationStage {
    match state {
        LegacyChapterImportState::Staged => LegacyChapterMigrationStage::Staged,
        LegacyChapterImportState::Verified => LegacyChapterMigrationStage::Verified,
        LegacyChapterImportState::Imported => LegacyChapterMigrationStage::Imported,
        LegacyChapterImportState::Corrupt => LegacyChapterMigrationStage::Blocked,
        LegacyChapterImportState::Discarded => LegacyChapterMigrationStage::Discarded,
    }
}

const fn failure_code(error: &StorageError) -> LegacyChapterMigrationFailureCode {
    match error {
        StorageError::SourceChanged => LegacyChapterMigrationFailureCode::SourceChanged,
        StorageError::BackupConflict => LegacyChapterMigrationFailureCode::BackupConflict,
        StorageError::ChapterImportConflict => LegacyChapterMigrationFailureCode::ImportConflict,
        StorageError::ChapterImportNotFound => LegacyChapterMigrationFailureCode::ImportNotFound,
        StorageError::CutoverAlreadyAuthoritative => {
            LegacyChapterMigrationFailureCode::AlreadyAuthoritative
        }
        StorageError::Interrupted => LegacyChapterMigrationFailureCode::Interrupted,
        StorageError::UnsupportedLegacySource
        | StorageError::NewerLegacyChapterSchema { .. }
        | StorageError::InvalidLegacyRecord { .. }
        | StorageError::InvalidChapterArtifact
        | StorageError::ImportLimitExceeded { .. } => {
            LegacyChapterMigrationFailureCode::SourceInvalid
        }
        StorageError::Io { .. } | StorageError::Sqlite { .. } => {
            LegacyChapterMigrationFailureCode::StorageUnavailable
        }
        _ => LegacyChapterMigrationFailureCode::TargetBlocked,
    }
}
