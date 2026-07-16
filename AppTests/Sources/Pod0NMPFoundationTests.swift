import Foundation
import XCTest
@testable import Podcastr

final class Pod0NMPFoundationTests: XCTestCase {
    func testConfigurationClassifiesOnlyOperatorRelayAndIgnoresInvalidValues() {
        let configuration = Pod0NMPConfiguration(
            storeURL: URL(fileURLWithPath: "/tmp/pod0-nmp-test.redb"),
            indexerRelays: [" wss://indexer.example ", "https://not-a-relay.example"],
            operatorRelay: "wss://operator.example",
            fallbackRelays: ["wss://fallback.example", "wss://fallback.example"]
        )

        XCTAssertEqual(configuration.indexerRelays, ["wss://indexer.example"])
        XCTAssertEqual(configuration.appRelays, ["wss://operator.example"])
        XCTAssertEqual(configuration.fallbackRelays, ["wss://fallback.example"])
        XCTAssertEqual(configuration.limits.maxRelays, 12)
        XCTAssertEqual(configuration.limits.maxNativeTasks, 16)
        XCTAssertEqual(configuration.limits.maxAuthCapabilities, 8)
        XCTAssertEqual(configuration.nmpRevision, Pod0NMPBuild.testedRevision)
    }

    func testStagedRelayProducesNextConstructionPolicyWithoutMutatingCurrentConfig() {
        let original = Pod0NMPConfiguration(
            storeURL: URL(fileURLWithPath: "/tmp/pod0-nmp-test.redb"),
            indexerRelays: ["wss://indexer.example"],
            operatorRelay: "wss://old.example",
            fallbackRelays: []
        )

        let staged = original.stagingOperatorRelay("wss://new.example")

        XCTAssertEqual(original.appRelays, ["wss://old.example"])
        XCTAssertEqual(staged.appRelays, ["wss://new.example"])
        XCTAssertEqual(staged.storePath, original.storePath)
    }

    func testProcessCannotLeaseSameCanonicalStoreTwice() throws {
        let first = try Pod0NMPStoreLease.acquire(path: "/tmp/pod0-nmp/../pod0-nmp/store.redb")
        defer { first.release() }

        XCTAssertThrowsError(try Pod0NMPStoreLease.acquire(path: "/tmp/pod0-nmp/store.redb")) { error in
            guard case Pod0NMPCompositionError.storeAlreadyOwned = error else {
                return XCTFail("Expected storeAlreadyOwned, got \(error)")
            }
        }
        first.release()
        let replacement = try Pod0NMPStoreLease.acquire(path: "/tmp/pod0-nmp/store.redb")
        replacement.release()
    }

    func testStoreLayoutUsesApplicationSupportShapeAndExcludesRootFromBackup() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-nmp-layout-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let layout = Pod0NMPStoreLayout(rootDirectory: root)

        try layout.prepare()

        XCTAssertTrue(FileManager.default.fileExists(atPath: root.path))
        XCTAssertEqual(layout.storeURL.lastPathComponent, "canonical.redb")
        XCTAssertEqual(layout.backupPolicy, .excludedFromDeviceBackup)
        XCTAssertEqual(
            try root.resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup,
            true
        )
    }

    func testDiagnosticsSummaryDoesNotClaimGlobalCompleteness() {
        let configuration = Pod0NMPConfiguration(
            storeURL: URL(fileURLWithPath: "/tmp/store"),
            indexerRelays: [],
            operatorRelay: nil,
            fallbackRelays: []
        )
        let snapshot = Pod0NMPDiagnosticsSnapshot(
            configuration: configuration,
            relays: [],
            authSessions: [],
            uncoveredAuthorCount: 0,
            transportDegraded: nil,
            identityBlocker: nil
        )

        XCTAssertTrue(snapshot.supportSummary.contains("scoped coverage facts"))
        XCTAssertFalse(snapshot.supportSummary.lowercased().contains("fully synced"))
        XCTAssertFalse(snapshot.supportSummary.lowercased().contains("complete"))
    }
}

