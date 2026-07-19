import CryptoKit
import Foundation
import Pod0Core
import os.log

enum SharedLibraryBootstrapOutcome {
    case ready(SharedLibraryClient)
    case authoritativeUnavailable(reason: String)
}

enum SharedLibraryBootstrap {
    private static let logger = Logger.app("SharedLibraryBootstrap")

    @MainActor
    static func run(
        persistence: Persistence,
        feedHost: any CoreFeedHosting = CoreFeedHost()
    ) -> SharedLibraryBootstrapOutcome {
        let target = persistence.sharedCoreStoreURL
        do {
            let observedAt = UnixTimestampMilliseconds(date: Date()).value
            let storeID = stableID("pod0-core-store:\(target.standardizedFileURL.path)")
            _ = try prepareSharedListeningStore(
                targetPath: target.path,
                schemaBackupPath: persistence.sharedCoreSchemaBackupURL.path,
                migrationId: storeID,
                observedAtMilliseconds: observedAt
            )
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
                    targetSchemaBackupPath: persistence.sharedCoreSchemaBackupURL.path,
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
                    targetSchemaBackupPath: persistence.sharedCoreSchemaBackupURL.path,
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
            let facade = try Pod0Facade.open(storePath: target.path)
            let client = SharedLibraryClient(facade: facade, feedHost: feedHost)
            client.start()
            logger.info("Shared Rust library is authoritative at \(target.path, privacy: .public)")
            return .ready(client)
        } catch {
            let detail = String(describing: error)
            logger.error("Shared library bootstrap failed: \(detail, privacy: .public)")
            return .authoritativeUnavailable(reason: detail)
        }
    }

    private static func stableID(_ seed: String) -> CommandId {
        let digest = Array(SHA256.hash(data: Data(seed.utf8)))
        let high = digest[0..<8].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        let low = digest[8..<16].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        return CommandId(high: high, low: low)
    }
}

private enum SharedLibraryBootstrapError: Error {
    case verificationFailed
}

#if DEBUG
extension AppStateStore {
    /// Builds an isolated legacy-import fixture for SwiftUI previews. Release
    /// code has no writer or convenience initializer for listening state.
    static func previewStore(importing state: AppState, name: String) -> AppStateStore {
        let persistence = Persistence(
            fileURL: FileManager.default.temporaryDirectory.appendingPathComponent(
                "pod0-\(name)-preview-\(UUID().uuidString).json"
            )
        )
        _ = persistence.write(state, revision: 1)
        return AppStateStore(
            persistence: persistence,
            startSubscriptionRefresh: false
        )
    }
}
#endif
