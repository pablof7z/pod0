import Foundation
import Pod0Core

extension SharedLibraryBootstrap {
    static func stageAndCommitTranscripts(
        persistence: Persistence,
        target: URL,
        schemaBackup: URL,
        storeID: CommandId,
        observedAt: Int64,
        stage: inout SharedLibraryBootstrapStage
    ) throws {
        let source = persistence.episodeStore.fileURL
        let transcriptRoot = persistence.legacyTranscriptRootURL
        let backupRoot = persistence.legacyTranscriptBackupRootURL
        let plan = try inspectLegacyTranscriptSource(
            sourceDatabasePath: source.path,
            transcriptRootPath: transcriptRoot.path
        )
        var active = try readActiveLegacyTranscriptImport(targetPath: target.path)
        if let existing = active,
           existing.plan != plan || existing.state == .corrupt {
            _ = try discardStagedLegacyTranscriptImport(
                targetPath: target.path,
                importId: existing.importId,
                observedAtMilliseconds: observedAt
            )
            active = nil
        }
        let importID = active?.importId ?? CommandId(uuid: UUID())
        stage = .transcriptStaging
        let report = try stageLegacyTranscriptImport(
            sourceDatabasePath: source.path,
            transcriptRootPath: transcriptRoot.path,
            legacyBackupRootPath: backupRoot.path,
            targetPath: target.path,
            targetSchemaBackupPath: schemaBackup.path,
            expectedPlan: plan,
            importId: importID,
            targetStoreId: storeID,
            observedAtMilliseconds: observedAt
        )
        stage = .transcriptVerification
        let verification = try verifyStagedLegacyTranscriptImport(
            targetPath: target.path,
            legacyBackupRootPath: backupRoot.path,
            importId: importID,
            observedAtMilliseconds: observedAt
        )
        guard report.plan == plan,
              verification.report.plan == plan,
              verification.verifiedArtifactCount == plan.artifactCount
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
        stage = .transcriptCommit
        let committed = try commitStagedLegacyTranscriptImport(
            sourceDatabasePath: source.path,
            transcriptRootPath: transcriptRoot.path,
            targetPath: target.path,
            importId: importID,
            observedAtMilliseconds: observedAt
        )
        guard committed.state == .committed,
              try sharedTranscriptStoreIsAuthoritative(targetPath: target.path)
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
    }
}
