import Foundation
import Pod0Core

extension SharedLibraryBootstrap {
    static func stageAndCommitClips(
        persistence: Persistence,
        target: URL,
        schemaBackup: URL,
        storeID: CommandId,
        observedAt: Int64
    ) throws {
        let source = persistence.episodeStore.fileURL
        let plan = try inspectLegacyClipSource(sourcePath: source.path)
        let importID = stableID(
            "pod0-clip-import:\(plan.sourceHash):\(plan.sourceGeneration)"
        )
        let report = try stageLegacyClipImport(
            sourcePath: source.path,
            sourceBackupPath: persistence.legacyClipsBackupURL(for: plan).path,
            targetPath: target.path,
            targetSchemaBackupPath: schemaBackup.path,
            expectedPlan: plan,
            importId: importID,
            targetStoreId: storeID,
            observedAtMilliseconds: observedAt
        )
        let verification = try readStagedLegacyClipImport(
            targetPath: target.path,
            importId: importID
        )
        guard report.staged,
              verification.report.plan == plan,
              verification.clips.count == Int(plan.clipCount)
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
        _ = try commitStagedLegacyClipImport(
            sourcePath: source.path,
            targetPath: target.path,
            observedAtMilliseconds: observedAt
        )
    }
}
