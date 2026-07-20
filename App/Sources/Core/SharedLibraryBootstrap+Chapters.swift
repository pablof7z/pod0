import Foundation
import Pod0Core

extension SharedLibraryBootstrap {
    static func stageAndCommitChapters(
        persistence: Persistence,
        target: URL,
        schemaBackup: URL,
        storeID: CommandId,
        stage: inout SharedLibraryBootstrapStage
    ) throws {
        let source = persistence.episodeStore.fileURL
        let artifactRoot = persistence.legacyChapterArtifactRootURL
        let backupRoot = persistence.legacyChapterBackupRootURL
        let inspected = inspectLegacyChapterMigration(
            sourceDatabasePath: source.path,
            artifactRootPath: artifactRoot.path
        )
        guard inspected.stage == .inspected,
              let plan = inspected.plan,
              plan.blockedCount == 0
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }

        var active = readActiveLegacyChapterMigration(targetPath: target.path)
        if active.stage == .imported,
           let report = active.report,
           report.plan == plan {
            try reactivateImportedChapters(
                report: report,
                source: source,
                artifactRoot: artifactRoot,
                target: target,
                stage: &stage
            )
            return
        }
        if let report = active.report,
           active.stage != .imported,
           (report.plan != plan || active.stage == .blocked) {
            let discarded = discardStagedLegacyChapterImport(
                targetPath: target.path,
                importId: report.importId
            )
            guard discarded.stage == .discarded else {
                throw SharedLibraryBootstrapError.verificationFailed
            }
            active = readActiveLegacyChapterMigration(targetPath: target.path)
        }

        let importID: CommandId
        if let report = active.report,
           report.plan == plan,
           active.stage == .staged || active.stage == .verified {
            importID = report.importId
        } else {
            importID = stableID(
                "pod0-chapter-import:\(plan.sourceSelectionDigest.stableString):\(plan.sourceGeneration)"
            )
        }
        stage = .chapterStaging
        let staged = stageLegacyChapterImport(
            sourceDatabasePath: source.path,
            artifactRootPath: artifactRoot.path,
            legacyBackupRootPath: backupRoot.path,
            targetPath: target.path,
            targetSchemaBackupPath: schemaBackup.path,
            expectedPlan: plan,
            importId: importID,
            targetStoreId: storeID
        )
        guard staged.report?.plan == plan,
              staged.stage == .staged || staged.stage == .verified
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
        stage = .chapterVerification
        let verified = verifyStagedLegacyChapterImport(
            sourceDatabasePath: source.path,
            artifactRootPath: artifactRoot.path,
            legacyBackupRootPath: backupRoot.path,
            targetPath: target.path,
            importId: importID
        )
        guard verified.stage == .verified,
              verified.report?.plan == plan,
              verified.verification?.verifiedEvidenceCount == plan.evidenceCount,
              verified.verification?.verifiedArtifactCount == plan.canonicalArtifactCount
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
        stage = .chapterCommit
        let committed = commitStagedLegacyChapterImport(
            sourceDatabasePath: source.path,
            artifactRootPath: artifactRoot.path,
            targetPath: target.path,
            importId: importID
        )
        guard committed.stage == .imported,
              sharedChapterStoreIsAuthoritative(targetPath: target.path)
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
    }

    private static func reactivateImportedChapters(
        report: LegacyChapterImportReport,
        source: URL,
        artifactRoot: URL,
        target: URL,
        stage: inout SharedLibraryBootstrapStage
    ) throws {
        stage = .chapterCommit
        let committed = commitStagedLegacyChapterImport(
            sourceDatabasePath: source.path,
            artifactRootPath: artifactRoot.path,
            targetPath: target.path,
            importId: report.importId
        )
        guard committed.stage == .imported,
              sharedChapterStoreIsAuthoritative(targetPath: target.path)
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
    }
}
