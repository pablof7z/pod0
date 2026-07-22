import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedChapterRecoveryTests: XCTestCase {
    func testVerifiedChapterStageResumesAfterProcessDeathAndCommitsOnce() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        try SharedChapterRecoveryTestSupport.injectLegacyChapters(fixture)
        let storeID = try SharedTranscriptRecoveryTestSupport.prepareCorePrerequisites(fixture)
        let persistence = fixture.persistence
        let inspected = inspectLegacyChapterMigration(
            sourceDatabasePath: persistence.episodeStore.fileURL.path,
            artifactRootPath: persistence.legacyChapterArtifactRootURL.path
        )
        let plan = try XCTUnwrap(inspected.plan)
        XCTAssertEqual(inspected.stage, .inspected)
        XCTAssertEqual(plan.blockedCount, 0)
        XCTAssertEqual(plan.canonicalArtifactCount, 1)
        XCTAssertEqual(plan.selectedCount, 1)

        let importID = CommandId(high: 90, low: 4)
        let staged = stageLegacyChapterImport(
            sourceDatabasePath: persistence.episodeStore.fileURL.path,
            artifactRootPath: persistence.legacyChapterArtifactRootURL.path,
            legacyBackupRootPath: persistence.legacyChapterBackupRootURL.path,
            targetPath: persistence.sharedCoreStoreURL.path,
            targetSchemaBackupPath: persistence.sharedCoreSchemaBackupURL(
                targetVersion: sharedStoreSchemaVersion()
            ).path,
            expectedPlan: plan,
            importId: importID,
            targetStoreId: storeID
        )
        XCTAssertEqual(staged.stage, .staged)
        let verified = verifyStagedLegacyChapterImport(
            sourceDatabasePath: persistence.episodeStore.fileURL.path,
            artifactRootPath: persistence.legacyChapterArtifactRootURL.path,
            legacyBackupRootPath: persistence.legacyChapterBackupRootURL.path,
            targetPath: persistence.sharedCoreStoreURL.path,
            importId: importID
        )
        XCTAssertEqual(verified.stage, .verified)
        XCTAssertEqual(verified.verification?.verifiedArtifactCount, 1)
        XCTAssertEqual(verified.verification?.verifiedChapterCount, 1)
        XCTAssertFalse(sharedChapterStoreIsAuthoritative(
            targetPath: persistence.sharedCoreStoreURL.path
        ))

        let outcome = SharedLibraryBootstrap.run(
            persistence: persistence,
            legacyState: try persistence.load(),
            feedHost: QueuedCoreFeedHost([])
        )
        guard case .ready(let client) = outcome else {
            guard case .authoritativeUnavailable(let reason, let stage) = outcome else {
                return XCTFail("Expected verified chapter stage to resume")
            }
            return XCTFail("Expected chapter recovery, got \(stage.rawValue):\(reason)")
        }
        defer { client.shutdown() }
        let restored = try XCTUnwrap(
            client.authoritativeChapterReader.load(episodeID: fixture.episodeID)
        )
        XCTAssertEqual(restored.chapters.map(\.title), ["Recovered chapter"])
        XCTAssertEqual(restored.chapters.first?.summary, "Preserved summary")
        XCTAssertEqual(restored.adSegments, [])
        XCTAssertTrue(sharedChapterStoreIsAuthoritative(
            targetPath: persistence.sharedCoreStoreURL.path
        ))
        XCTAssertEqual(
            readActiveLegacyChapterMigration(targetPath: persistence.sharedCoreStoreURL.path).stage,
            .imported
        )
    }

    func testUnavailableCoreSuppressesNativeWritesAndPreservesLegacyChapterEvidence() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        try SharedChapterRecoveryTestSupport.injectLegacyChapters(fixture)
        try Data("not-a-sqlite-store".utf8).write(
            to: fixture.persistence.sharedCoreStoreURL,
            options: .atomic
        )
        let store = AppStateStore(
            persistence: fixture.persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(store.sharedLibrary)
        XCTAssertNotNil(store.sharedLibraryUnavailableReason)
        XCTAssertEqual(store.state.episodes.first?.chapters?.first?.title, "Recovered chapter")

        store.mutateState { state in
            state.episodes[0].title = "Must remain memory-only"
        }

        let reloaded = try fixture.persistence.load()
        XCTAssertEqual(reloaded.episodes.first?.title, "Transcript Recovery")
        XCTAssertEqual(reloaded.episodes.first?.chapters?.first?.title, "Recovered chapter")
        XCTAssertEqual(reloaded.episodes.first?.adSegments, [])
        let legacy = inspectLegacyChapterMigration(
            sourceDatabasePath: fixture.persistence.episodeStore.fileURL.path,
            artifactRootPath: fixture.persistence.legacyChapterArtifactRootURL.path
        )
        XCTAssertEqual(legacy.stage, .inspected)
        XCTAssertEqual(legacy.plan?.canonicalArtifactCount, 1)
        XCTAssertEqual(legacy.plan?.selectedCount, 1)
    }

    func testRelaunchAfterAuthorityDoesNotDecodeLegacyChapterAdjuncts() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        try SharedChapterRecoveryTestSupport.injectLegacyChapters(fixture)
        _ = try SharedTranscriptRecoveryTestSupport.prepareCorePrerequisites(fixture)
        let first = SharedLibraryBootstrap.run(
            persistence: fixture.persistence,
            legacyState: try fixture.persistence.load(),
            feedHost: QueuedCoreFeedHost([])
        )
        guard case .ready(let firstClient) = first else {
            return XCTFail("Expected initial chapter authority cutover")
        }
        firstClient.shutdown()
        XCTAssertEqual(
            try fixture.persistence.load().episodes.first?.chapters?.first?.title,
            "Recovered chapter"
        )

        let relaunched = AppStateStore(
            persistence: fixture.persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        defer { relaunched.sharedLibrary?.shutdown() }

        XCTAssertNotNil(relaunched.sharedLibrary)
        XCTAssertNil(relaunched.sharedLibraryUnavailableReason)
        XCTAssertNil(relaunched.state.episodes.first?.chapters)
        XCTAssertNil(relaunched.state.episodes.first?.adSegments)
    }

    func testTypedRollbackExportIsReplayableAndNoClobber() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        try SharedChapterRecoveryTestSupport.injectLegacyChapters(fixture)
        _ = try SharedTranscriptRecoveryTestSupport.prepareCorePrerequisites(fixture)
        let outcome = SharedLibraryBootstrap.run(
            persistence: fixture.persistence,
            legacyState: try fixture.persistence.load(),
            feedHost: QueuedCoreFeedHost([])
        )
        guard case .ready(let client) = outcome else {
            return XCTFail("Expected imported chapter store")
        }
        client.shutdown()
        let exportRoot = fixture.fileURL.deletingLastPathComponent().appendingPathComponent(
            "chapter-rollback-\(UUID().uuidString)",
            isDirectory: true
        )
        defer { try? FileManager.default.removeItem(at: exportRoot) }

        let first = exportLegacyChapterRollback(
            targetPath: fixture.persistence.sharedCoreStoreURL.path,
            legacyBackupRootPath: fixture.persistence.legacyChapterBackupRootURL.path,
            exportRootPath: exportRoot.path
        )
        let report = try XCTUnwrap(first.rollbackExport)
        XCTAssertEqual(first.stage, .imported)
        XCTAssertEqual(report.formatVersion, 1)
        XCTAssertEqual(report.coreSchemaVersion, sharedStoreSchemaVersion())
        XCTAssertEqual(report.artifactCount, 1)
        XCTAssertFalse(report.reusedExisting)
        let bundle = URL(fileURLWithPath: report.bundlePath, isDirectory: true)
        let replay = inspectLegacyChapterMigration(
            sourceDatabasePath: bundle.appendingPathComponent("source.sqlite").path,
            artifactRootPath: bundle.path
        )
        XCTAssertEqual(replay.stage, .inspected)
        XCTAssertEqual(replay.plan?.canonicalArtifactCount, 1)
        XCTAssertEqual(replay.plan?.selectedCount, 1)

        let second = exportLegacyChapterRollback(
            targetPath: fixture.persistence.sharedCoreStoreURL.path,
            legacyBackupRootPath: fixture.persistence.legacyChapterBackupRootURL.path,
            exportRootPath: exportRoot.path
        )
        XCTAssertTrue(second.rollbackExport?.reusedExisting == true)
        XCTAssertEqual(second.rollbackExport?.bundlePath, report.bundlePath)
        XCTAssertEqual(second.rollbackExport?.bundleDigest, report.bundleDigest)
    }
}
