import Pod0Core

enum SharedLibraryBootstrapFailureCode: String {
    case sourceChanged = "SourceChanged"
    case sourceInvalid = "SourceInvalid"
    case backupConflict = "BackupConflict"
    case importConflict = "ImportConflict"
    case importNotFound = "ImportNotFound"
    case alreadyAuthoritative = "AlreadyAuthoritative"
    case targetBlocked = "TargetBlocked"
    case interrupted = "Interrupted"
    case notAuthoritative = "NotAuthoritative"
    case schemaBlocked = "SchemaBlocked"
    case storageUnavailable = "StorageUnavailable"
    case verificationFailed = "VerificationFailed"
    case unexpected = "Unexpected"

    static func classify(_ error: any Error) -> Self {
        switch error {
        case LegacyClipMigrationError.SourceChanged,
             LegacyListeningMigrationError.SourceChanged,
             LegacyNoteMigrationError.SourceChanged,
             LegacyTranscriptMigrationError.SourceChanged,
             LegacyModelChapterWorkflowBackupError.sourceChanged:
            .sourceChanged
        case LegacyClipMigrationError.SourceInvalid,
             LegacyListeningMigrationError.SourceInvalid,
             LegacyNoteMigrationError.SourceInvalid,
             LegacyTranscriptMigrationError.SourceInvalid:
            .sourceInvalid
        case LegacyClipMigrationError.BackupConflict,
             LegacyListeningMigrationError.BackupConflict,
             LegacyNoteMigrationError.BackupConflict,
             LegacyTranscriptMigrationError.BackupConflict,
             LegacyModelChapterWorkflowBackupError.backupConflict:
            .backupConflict
        case LegacyClipMigrationError.ImportConflict,
             LegacyListeningMigrationError.ImportConflict,
             LegacyNoteMigrationError.ImportConflict,
             LegacyTranscriptMigrationError.ImportConflict:
            .importConflict
        case LegacyClipMigrationError.ImportNotFound,
             LegacyListeningMigrationError.ImportNotFound,
             LegacyNoteMigrationError.ImportNotFound,
             LegacyTranscriptMigrationError.ImportNotFound:
            .importNotFound
        case LegacyTranscriptMigrationError.AlreadyAuthoritative:
            .alreadyAuthoritative
        case LegacyClipMigrationError.TargetBlocked,
             LegacyListeningMigrationError.TargetBlocked,
             LegacyNoteMigrationError.TargetBlocked,
             LegacyTranscriptMigrationError.TargetBlocked:
            .targetBlocked
        case LegacyClipMigrationError.Interrupted,
             LegacyListeningMigrationError.Interrupted,
             LegacyNoteMigrationError.Interrupted,
             LegacyTranscriptMigrationError.Interrupted:
            .interrupted
        case FacadeOpenError.NotAuthoritative:
            .notAuthoritative
        case FacadeOpenError.SchemaBlocked:
            .schemaBlocked
        case LegacyClipMigrationError.StorageUnavailable,
             LegacyListeningMigrationError.StorageUnavailable,
             LegacyNoteMigrationError.StorageUnavailable,
             LegacyTranscriptMigrationError.StorageUnavailable,
             FacadeOpenError.StorageUnavailable:
            .storageUnavailable
        case SharedLibraryBootstrapError.verificationFailed:
            .verificationFailed
        case LegacyModelChapterWorkflowBackupError.backupMissing,
             LegacyModelChapterWorkflowBackupError.invalidBackup,
             LegacyModelChapterWorkflowBackupError.durabilityFailed:
            .verificationFailed
        default:
            .unexpected
        }
    }
}
