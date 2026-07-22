import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedTranscriptRecoveryTests: XCTestCase {
    func testVerifiedTranscriptStageResumesAfterProcessDeathAndCommitsOnce() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        let original = try SharedTranscriptRecoveryTestSupport.seedLegacyTranscript(fixture)
        let storeID = try SharedTranscriptRecoveryTestSupport.prepareCorePrerequisites(fixture)
        let persistence = fixture.persistence
        let plan = try inspectLegacyTranscriptSource(
            sourceDatabasePath: persistence.episodeStore.fileURL.path,
            transcriptRootPath: persistence.legacyTranscriptRootURL.path
        )
        let importID = CommandId(high: 90, low: 3)
        let report = try stageLegacyTranscriptImport(
            sourceDatabasePath: persistence.episodeStore.fileURL.path,
            transcriptRootPath: persistence.legacyTranscriptRootURL.path,
            legacyBackupRootPath: persistence.legacyTranscriptBackupRootURL.path,
            targetPath: persistence.sharedCoreStoreURL.path,
            targetSchemaBackupPath: persistence.sharedCoreSchemaBackupURL(
                targetVersion: sharedStoreSchemaVersion()
            ).path,
            expectedPlan: plan,
            importId: importID,
            targetStoreId: storeID,
            observedAtMilliseconds: 1_700_000_000_101
        )
        XCTAssertEqual(report.state, .staged)
        let verified = try verifyStagedLegacyTranscriptImport(
            targetPath: persistence.sharedCoreStoreURL.path,
            legacyBackupRootPath: persistence.legacyTranscriptBackupRootURL.path,
            importId: importID,
            observedAtMilliseconds: 1_700_000_000_102
        )
        XCTAssertEqual(verified.report.state, .verified)
        XCTAssertFalse(try sharedTranscriptStoreIsAuthoritative(
            targetPath: persistence.sharedCoreStoreURL.path
        ))
        let outcome = SharedLibraryBootstrap.run(
            persistence: persistence,
            legacyState: try persistence.load(),
            feedHost: QueuedCoreFeedHost([])
        )
        guard case .ready(let client) = outcome else {
            guard case .authoritativeUnavailable(let reason, let stage) = outcome else {
                return XCTFail("Expected verified transcript stage to resume")
            }
            return XCTFail(
                "Expected verified transcript stage to resume, got \(stage.rawValue):\(reason)"
            )
        }
        defer { client.shutdown() }
        let restored = try XCTUnwrap(
            client.authoritativeTranscriptReader.loadThrowing(episodeID: fixture.episodeID)
        )
        XCTAssertEqual(restored.segments[0].text, "Original durable transcript")
        XCTAssertTrue(try sharedTranscriptStoreIsAuthoritative(
            targetPath: persistence.sharedCoreStoreURL.path
        ))
        XCTAssertNil(try readActiveLegacyTranscriptImport(
            targetPath: persistence.sharedCoreStoreURL.path
        ))
        XCTAssertEqual(
            try Data(contentsOf: persistence.legacyTranscriptRootURL.appendingPathComponent(
                "\(fixture.episodeID.uuidString).json"
            )),
            original
        )
    }

    func testUnavailableCoreNeverFallsBackToLegacyReadinessOrTranscriptFile() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        let original = try SharedTranscriptRecoveryTestSupport.seedLegacyTranscript(fixture)
        try SharedTranscriptRecoveryTestSupport.injectLegacyReadyState(fixture)
        XCTAssertEqual(
            try fixture.persistence.load().episodes.first?.transcriptState,
            .ready(source: .publisher)
        )
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
        XCTAssertNil(store.transcriptReader.load(episodeID: fixture.episodeID))
        XCTAssertEqual(
            try Data(contentsOf: fixture.persistence.legacyTranscriptRootURL.appendingPathComponent(
                "\(fixture.episodeID.uuidString).json"
            )),
            original
        )
        let legacyPlan = try inspectLegacyTranscriptSource(
            sourceDatabasePath: fixture.persistence.episodeStore.fileURL.path,
            transcriptRootPath: fixture.persistence.legacyTranscriptRootURL.path
        )
        XCTAssertEqual(legacyPlan.artifactCount, 1)
        XCTAssertEqual(legacyPlan.selectedCount, 1)
    }

    func testTypedRollbackExportIncludesHistoryAndIsNoClobber() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        let store = AppStateStore(
            persistence: fixture.persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        let client = try XCTUnwrap(store.sharedLibrary)
        defer { client.shutdown() }
        let first = try client.submitTranscriptObservation(
            SharedTranscriptRecoveryTestSupport.transcript(
                episodeID: fixture.episodeID,
                revision: "audio-v1"
            ),
            context: SharedTranscriptRecoveryTestSupport.context(
                podcastID: fixture.podcastID,
                revision: "audio-v1"
            )
        )
        _ = try client.submitTranscriptObservation(
            SharedTranscriptRecoveryTestSupport.transcript(
                episodeID: fixture.episodeID,
                revision: "audio-v2"
            ),
            context: SharedTranscriptRecoveryTestSupport.context(
                podcastID: fixture.podcastID,
                revision: "audio-v2"
            ),
            expectedSelectionRevision: first.receipt.selectionRevision
        )
        let exportRoot = fixture.fileURL.deletingLastPathComponent().appendingPathComponent(
            "rollback-\(UUID().uuidString)",
            isDirectory: true
        )
        defer { try? FileManager.default.removeItem(at: exportRoot) }

        let report = try exportLegacyTranscriptRollback(
            targetPath: fixture.persistence.sharedCoreStoreURL.path,
            exportRootPath: exportRoot.path
        )
        XCTAssertEqual(report.coreSchemaVersion, sharedStoreSchemaVersion())
        XCTAssertEqual(report.artifactCount, 2)
        XCTAssertEqual(report.selectedCount, 1)
        XCTAssertFalse(report.reusedExisting)
        let plan = try inspectLegacyTranscriptSource(
            sourceDatabasePath: URL(fileURLWithPath: report.bundlePath)
                .appendingPathComponent("transcript-selection.sqlite").path,
            transcriptRootPath: URL(fileURLWithPath: report.bundlePath)
                .appendingPathComponent("transcripts", isDirectory: true).path
        )
        XCTAssertEqual(plan.artifactCount, 2)
        XCTAssertEqual(plan.selectedCount, 1)
        XCTAssertTrue(try exportLegacyTranscriptRollback(
            targetPath: fixture.persistence.sharedCoreStoreURL.path,
            exportRootPath: exportRoot.path
        ).reusedExisting)
    }
}
