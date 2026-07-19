use pod0_storage::{
    StorageError, TranscriptBackupEvidence, TranscriptImportPlan, TranscriptImportReport,
    TranscriptImportState, TranscriptImportVerification, TranscriptRollbackExportReport,
};

use crate::transcript_migration::{
    LegacyTranscriptBackupEvidence, LegacyTranscriptImportPlan, LegacyTranscriptImportReport,
    LegacyTranscriptImportState, LegacyTranscriptImportVerification,
    LegacyTranscriptMigrationError, LegacyTranscriptRollbackExportReport,
    LegacyTranscriptSourceKind,
};

impl From<pod0_storage::LegacyTranscriptSourceKind> for LegacyTranscriptSourceKind {
    fn from(value: pod0_storage::LegacyTranscriptSourceKind) -> Self {
        match value {
            pod0_storage::LegacyTranscriptSourceKind::ArtifactSqliteV0 => Self::ArtifactSqliteV0,
            pod0_storage::LegacyTranscriptSourceKind::ArtifactSqliteV1 => Self::ArtifactSqliteV1,
        }
    }
}

impl From<LegacyTranscriptSourceKind> for pod0_storage::LegacyTranscriptSourceKind {
    fn from(value: LegacyTranscriptSourceKind) -> Self {
        match value {
            LegacyTranscriptSourceKind::ArtifactSqliteV0 => Self::ArtifactSqliteV0,
            LegacyTranscriptSourceKind::ArtifactSqliteV1 => Self::ArtifactSqliteV1,
        }
    }
}

impl From<TranscriptImportState> for LegacyTranscriptImportState {
    fn from(value: TranscriptImportState) -> Self {
        match value {
            TranscriptImportState::Staged => Self::Staged,
            TranscriptImportState::Verified => Self::Verified,
            TranscriptImportState::Committed => Self::Committed,
            TranscriptImportState::Corrupt => Self::Corrupt,
            TranscriptImportState::Discarded => Self::Discarded,
        }
    }
}

impl From<TranscriptImportPlan> for LegacyTranscriptImportPlan {
    fn from(value: TranscriptImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_generation: value.source_generation,
            source_database_digest: value.source_database_digest,
            source_selection_digest: value.source_selection_digest,
            artifact_count: value.artifact_count,
            selected_count: value.selected_count,
        }
    }
}

impl From<LegacyTranscriptImportPlan> for TranscriptImportPlan {
    fn from(value: LegacyTranscriptImportPlan) -> Self {
        Self {
            source_kind: value.source_kind.into(),
            source_generation: value.source_generation,
            source_database_digest: value.source_database_digest,
            source_selection_digest: value.source_selection_digest,
            artifact_count: value.artifact_count,
            selected_count: value.selected_count,
        }
    }
}

impl From<TranscriptBackupEvidence> for LegacyTranscriptBackupEvidence {
    fn from(value: TranscriptBackupEvidence) -> Self {
        Self {
            database_digest: value.database_digest,
            database_byte_count: value.database_byte_count,
            artifact_count: value.artifact_count,
            artifact_byte_count: value.artifact_byte_count,
            reused_database: value.reused_database,
            reused_artifacts: value.reused_artifacts,
        }
    }
}

impl From<TranscriptImportReport> for LegacyTranscriptImportReport {
    fn from(value: TranscriptImportReport) -> Self {
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

impl From<TranscriptImportVerification> for LegacyTranscriptImportVerification {
    fn from(value: TranscriptImportVerification) -> Self {
        Self {
            report: value.report.into(),
            verified_artifact_count: value.verified_artifact_count,
            verified_segment_count: value.verified_segment_count,
            verified_word_count: value.verified_word_count,
        }
    }
}

impl TryFrom<TranscriptRollbackExportReport> for LegacyTranscriptRollbackExportReport {
    type Error = LegacyTranscriptMigrationError;

    fn try_from(value: TranscriptRollbackExportReport) -> Result<Self, Self::Error> {
        Ok(Self {
            bundle_path: value
                .bundle_path
                .into_os_string()
                .into_string()
                .map_err(|_| LegacyTranscriptMigrationError::StorageUnavailable)?,
            core_schema_version: value.core_schema_version,
            transcript_revision: value.transcript_revision,
            artifact_count: value.artifact_count,
            selected_count: value.selected_count,
            reused_existing: value.reused_existing,
        })
    }
}

impl From<StorageError> for LegacyTranscriptMigrationError {
    fn from(value: StorageError) -> Self {
        match value {
            StorageError::SourceChanged => Self::SourceChanged,
            StorageError::BackupConflict => Self::BackupConflict,
            StorageError::TranscriptImportConflict
            | StorageError::TranscriptCommandConflict
            | StorageError::TranscriptRevisionConflict => Self::ImportConflict,
            StorageError::TranscriptImportNotFound | StorageError::EntityNotFound => {
                Self::ImportNotFound
            }
            StorageError::CutoverAlreadyAuthoritative => Self::AlreadyAuthoritative,
            StorageError::Interrupted => Self::Interrupted,
            StorageError::UnsupportedLegacySource
            | StorageError::NewerLegacyTranscriptSchema { .. }
            | StorageError::InvalidLegacyRecord { .. }
            | StorageError::InvalidTranscriptArtifact
            | StorageError::ImportLimitExceeded { .. } => Self::SourceInvalid,
            StorageError::Io { .. } | StorageError::Sqlite { .. } => Self::StorageUnavailable,
            _ => Self::TargetBlocked,
        }
    }
}
