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
    /// The store is ahead of this build. No relaunch fixes this — downgrade is
    /// refused, so the store is never rewritten back down.
    case storeNewerThanApp = "StoreNewerThanApp"
    /// A migration started and did not finish. A backup was taken first.
    case migrationFailed = "MigrationFailed"
    /// The store is corrupt, or belongs to another application.
    case storeUnreadable = "StoreUnreadable"
    case storageUnavailable = "StorageUnavailable"
    case verificationFailed = "VerificationFailed"
    case unexpected = "Unexpected"

    static func classify(_ error: any Error) -> Self {
        switch error {
        case LegacyClipMigrationError.SourceChanged,
             LegacyListeningMigrationError.SourceChanged,
             LegacyNoteMigrationError.SourceChanged,
             LegacyTranscriptMigrationError.SourceChanged,
             LegacyDownloadWorkflowBackupError.sourceChanged:
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
             LegacyWorkflowBackupError.backupConflict:
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
        case FacadeOpenError.SchemaBlocked(reason: .storeNewerThanApp):
            .storeNewerThanApp
        case FacadeOpenError.SchemaBlocked(reason: .migrationFailed):
            .migrationFailed
        case FacadeOpenError.SchemaBlocked(reason: .storeUnreadable):
            .storeUnreadable
        case LegacyClipMigrationError.StorageUnavailable,
             LegacyListeningMigrationError.StorageUnavailable,
             LegacyNoteMigrationError.StorageUnavailable,
             LegacyTranscriptMigrationError.StorageUnavailable,
             FacadeOpenError.StorageUnavailable:
            .storageUnavailable
        case SharedLibraryBootstrapError.verificationFailed:
            .verificationFailed
        case LegacyWorkflowBackupError.backupMissing,
             LegacyWorkflowBackupError.invalidBackup,
             LegacyWorkflowBackupError.durabilityFailed,
             LegacyDownloadWorkflowBackupError.backupMissing,
             LegacyDownloadWorkflowBackupError.backupCorrupt,
             LegacyDownloadWorkflowCutoverError.verificationFailed:
            .verificationFailed
        default:
            .unexpected
        }
    }
}
