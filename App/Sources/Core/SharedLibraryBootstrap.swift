import CryptoKit
import Foundation
import Pod0Core
import os.log

enum SharedLibraryBootstrapOutcome {
    case ready(SharedLibraryClient)
    case authoritativeUnavailable(reason: String, stage: SharedLibraryBootstrapStage)
}

enum SharedLibraryBootstrapStage: String {
    case storePreparation
    case listening
    case notes
    case clips
    case transcriptInspection
    case transcriptStaging
    case transcriptVerification
    case transcriptCommit
    case facade
}

enum SharedLibraryBootstrap {
    private static let logger = Logger.app("SharedLibraryBootstrap")

    @MainActor
    static func run(
        persistence: Persistence,
        feedHost: any CoreFeedHosting = CoreFeedHost()
    ) -> SharedLibraryBootstrapOutcome {
        persistence.withSharedArtifactMigrationLock {
            runLocked(persistence: persistence, feedHost: feedHost)
        }
    }

    @MainActor
    private static func runLocked(
        persistence: Persistence,
        feedHost: any CoreFeedHosting
    ) -> SharedLibraryBootstrapOutcome {
        let target = persistence.sharedCoreStoreURL
        var stage = SharedLibraryBootstrapStage.storePreparation
        do {
            let observedAt = UnixTimestampMilliseconds(date: Date()).value
            let schemaBackup = persistence.sharedCoreSchemaBackupURL(
                targetVersion: sharedStoreSchemaVersion()
            )
            let storeID = stableID("pod0-core-store:\(target.standardizedFileURL.path)")
            _ = try prepareSharedListeningStore(
                targetPath: target.path,
                schemaBackupPath: schemaBackup.path,
                migrationId: storeID,
                observedAtMilliseconds: observedAt
            )
            stage = .listening
            do {
                _ = try commitStagedLegacyListeningImport(
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            } catch LegacyListeningMigrationError.ImportNotFound {
                let plan = try inspectLegacyListeningSource(
                    sourcePath: persistence.episodeStore.fileURL.path
                )
                let importID = stableID(
                    "pod0-listening-import:\(plan.sourceHash):\(plan.sourceGeneration)"
                )
                let report = try stageLegacyListeningImport(
                    sourcePath: persistence.episodeStore.fileURL.path,
                    sourceBackupPath: persistence.legacyListeningBackupURL.path,
                    targetPath: target.path,
                    targetSchemaBackupPath: schemaBackup.path,
                    expectedPlan: plan,
                    importId: importID,
                    targetStoreId: storeID,
                    observedAtMilliseconds: observedAt
                )
                let verification = try readStagedLegacyListeningImport(
                    targetPath: target.path,
                    importId: importID
                )
                guard report.staged,
                      verification.report.plan == plan,
                      verification.snapshot.podcasts.count == Int(plan.podcastCount),
                      verification.snapshot.subscriptions.count == Int(plan.subscriptionCount),
                      verification.snapshot.episodes.count == Int(plan.episodeCount)
                else {
                    throw SharedLibraryBootstrapError.verificationFailed
                }
                _ = try commitStagedLegacyListeningImport(
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            }
            stage = .notes
            do {
                _ = try commitStagedLegacyNoteImport(
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            } catch LegacyNoteMigrationError.ImportNotFound {
                let plan = try inspectLegacyNoteSource(
                    sourcePath: persistence.episodeStore.fileURL.path
                )
                let importID = stableID(
                    "pod0-note-import:\(plan.sourceHash):\(plan.sourceGeneration)"
                )
                let report = try stageLegacyNoteImport(
                    sourcePath: persistence.episodeStore.fileURL.path,
                    sourceBackupPath: persistence.legacyNotesBackupURL.path,
                    targetPath: target.path,
                    targetSchemaBackupPath: schemaBackup.path,
                    expectedPlan: plan,
                    importId: importID,
                    targetStoreId: storeID,
                    observedAtMilliseconds: observedAt
                )
                let verification = try readStagedLegacyNoteImport(
                    targetPath: target.path,
                    importId: importID
                )
                guard report.staged,
                      verification.report.plan == plan,
                      verification.notes.count == Int(plan.noteCount)
                else {
                    throw SharedLibraryBootstrapError.verificationFailed
                }
                _ = try commitStagedLegacyNoteImport(
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            }
            persistence.activateSharedNoteAuthority()
            stage = .clips
            do {
                _ = try commitStagedLegacyClipImport(
                    sourcePath: persistence.episodeStore.fileURL.path,
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            } catch LegacyClipMigrationError.ImportNotFound {
                try stageAndCommitClips(
                    persistence: persistence,
                    target: target,
                    schemaBackup: schemaBackup,
                    storeID: storeID,
                    observedAt: observedAt
                )
            } catch LegacyClipMigrationError.SourceChanged {
                try stageAndCommitClips(
                    persistence: persistence,
                    target: target,
                    schemaBackup: schemaBackup,
                    storeID: storeID,
                    observedAt: observedAt
                )
            }
            persistence.activateSharedClipAuthority()
            stage = .transcriptInspection
            if try !sharedTranscriptStoreIsAuthoritative(targetPath: target.path) {
                try stageAndCommitTranscripts(
                    persistence: persistence,
                    target: target,
                    schemaBackup: schemaBackup,
                    storeID: storeID,
                    observedAt: observedAt,
                    stage: &stage
                )
            }
            stage = .facade
            let facade = try Pod0Facade.open(storePath: target.path)
            let client = SharedLibraryClient(facade: facade, feedHost: feedHost)
            client.start()
            logger.info("Shared Rust library is authoritative at \(target.path, privacy: .public)")
            return .ready(client)
        } catch {
            let code = SharedLibraryBootstrapFailureCode.classify(error)
            logger.error("Shared library bootstrap failed at \(stage.rawValue, privacy: .public): \(code.rawValue, privacy: .public)")
            return .authoritativeUnavailable(reason: code.rawValue, stage: stage)
        }
    }

    private static func stageAndCommitTranscripts(
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

    private static func stageAndCommitClips(
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

    private static func stableID(_ seed: String) -> CommandId {
        let digest = Array(SHA256.hash(data: Data(seed.utf8)))
        let high = digest[0..<8].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        let low = digest[8..<16].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        return CommandId(high: high, low: low)
    }
}

enum SharedLibraryBootstrapError: Error {
    case verificationFailed
}
