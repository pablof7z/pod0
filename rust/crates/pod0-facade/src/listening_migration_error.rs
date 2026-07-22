use pod0_storage::StorageError;

use crate::LegacyListeningMigrationError;

impl From<StorageError> for LegacyListeningMigrationError {
    fn from(value: StorageError) -> Self {
        match value {
            StorageError::SourceChanged => Self::SourceChanged,
            StorageError::BackupConflict => Self::BackupConflict,
            StorageError::ImportConflict
            | StorageError::CutoverAlreadyAuthoritative
            | StorageError::CommandConflict => Self::ImportConflict,
            StorageError::ImportNotFound | StorageError::EntityNotFound => Self::ImportNotFound,
            StorageError::Interrupted => Self::Interrupted,
            StorageError::UnsupportedLegacySource
            | StorageError::InvalidLegacyRecord { .. }
            | StorageError::ImportLimitExceeded { .. } => Self::SourceInvalid,
            StorageError::UnsupportedTarget { .. }
            | StorageError::DowngradeForbidden { .. }
            | StorageError::NewerSchema { .. }
            | StorageError::ForeignDatabase
            | StorageError::CorruptSchema { .. }
            | StorageError::CutoverNotAuthoritative
            | StorageError::RevisionConflict
            | StorageError::InvalidNote
            | StorageError::InvalidClip
            | StorageError::InvalidTranscriptArtifact
            | StorageError::TranscriptCommandConflict
            | StorageError::TranscriptNotFound
            | StorageError::TranscriptRevisionConflict
            | StorageError::TranscriptImportConflict
            | StorageError::TranscriptImportNotFound
            | StorageError::NewerLegacyTranscriptSchema { .. }
            | StorageError::InvalidChapterArtifact
            | StorageError::ChapterCommandConflict
            | StorageError::ChapterRevisionConflict
            | StorageError::ChapterImportConflict
            | StorageError::ChapterImportNotFound
            | StorageError::ChapterWorkflowConflict
            | StorageError::ChapterWorkflowNotFound
            | StorageError::NewerLegacyChapterSchema { .. }
            | StorageError::FailedMigration { .. }
            | StorageError::EvidenceCommandConflict
            | StorageError::EvidenceNotFound
            | StorageError::EvidenceNotVerified
            | StorageError::EvidenceGenerationSelected
            | StorageError::EvidenceEpisodeMismatch
            | StorageError::InvalidEvidenceArtifact
            | StorageError::InvalidRecallConfiguration
            | StorageError::RecallConfigurationNotFound
            | StorageError::NewerEvidenceSchema { .. }
            | StorageError::DownloadCommandConflict
            | StorageError::DownloadWorkflowConflict
            | StorageError::DownloadWorkflowNotFound
            | StorageError::DownloadRequestNotFound
            | StorageError::InvalidDownloadArtifact
            | StorageError::StaleDownloadAttempt => Self::TargetBlocked,
            StorageError::Io { .. } | StorageError::Sqlite { .. } => Self::StorageUnavailable,
        }
    }
}
