import Foundation
import XCTest
@testable import Podcastr

#if canImport(NMP)
import NMP
#endif

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
            layout.fileProtectionPolicy,
            .completeUntilFirstUserAuthentication
        )
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

    #if canImport(NMP)
    func testRealPinnedCompositionOwnsResetsAndReopensCanonicalStore() async throws {
        let root = temporaryRoot(named: "lifecycle")
        defer { try? FileManager.default.removeItem(at: root) }
        let layout = Pod0NMPStoreLayout(rootDirectory: root)
        let configuration = offlineConfiguration(layout: layout)
        let first = try Pod0NMPComposition(configuration: configuration, layout: layout)
        defer { first.shutdown() }

        XCTAssertEqual(configuration.nmpRevision, Pod0NMPBuild.testedRevision)
        XCTAssertEqual(configuration.storePath, layout.storeURL.standardizedFileURL.path)
        XCTAssertTrue(FileManager.default.fileExists(atPath: layout.storeURL.path))
        XCTAssertEqual(
            try root.resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup,
            true
        )

        XCTAssertThrowsError(
            try Pod0NMPComposition(configuration: configuration, layout: layout)
        ) { error in
            XCTAssertEqual(
                error as? Pod0NMPCompositionError,
                .storeAlreadyOwned(configuration.storePath)
            )
        }
        XCTAssertThrowsError(try first.resetStoreAfterShutdown()) { error in
            XCTAssertEqual(error as? Pod0NMPCompositionError, .engineStillRunning)
        }
        XCTAssertThrowsError(try NMPEngine.resetPersistentStore(at: configuration.storePath)) {
            guard case .storeStillOpen(let path) = $0 as? NMPError else {
                return XCTFail("Expected NMP storeStillOpen, got \($0)")
            }
            XCTAssertEqual(path, configuration.storePath)
        }

        first.shutdown()
        XCTAssertThrowsError(try first.stageOperatorRelay("wss://next.example")) { error in
            XCTAssertEqual(error as? Pod0NMPCompositionError, .engineShutdown)
        }

        let reopened = try Pod0NMPComposition(configuration: configuration, layout: layout)
        let restartedSnapshot = await firstSnapshot(from: try reopened.diagnostics())
        XCTAssertNotNil(restartedSnapshot, "Offline restart must immediately expose diagnostics")
        XCTAssertEqual(restartedSnapshot?.relays, [])
        XCTAssertEqual(restartedSnapshot?.authSessions, [])
        reopened.shutdown()

        try reopened.resetStoreAfterShutdown()
        XCTAssertFalse(FileManager.default.fileExists(atPath: layout.storeURL.path))

        let clean = try Pod0NMPComposition(configuration: configuration, layout: layout)
        XCTAssertTrue(FileManager.default.fileExists(atPath: layout.storeURL.path))
        clean.shutdown()
        try clean.resetStoreAfterShutdown()
    }

    func testRealDiagnosticsCancellationReturnsCapacityAcrossOfflineRestart() async throws {
        let root = temporaryRoot(named: "diagnostics")
        defer { try? FileManager.default.removeItem(at: root) }
        let layout = Pod0NMPStoreLayout(rootDirectory: root)
        let configuration = offlineConfiguration(
            layout: layout,
            limits: .init(maxRelays: 1, maxNativeTasks: 1, maxAuthCapabilities: 1)
        )
        let composition = try Pod0NMPComposition(configuration: configuration, layout: layout)

        var observation: AsyncStream<Pod0NMPDiagnosticsSnapshot>? = try composition.diagnostics()
        let initial = await firstSnapshot(from: try XCTUnwrap(observation))
        XCTAssertNotNil(initial, "Pinned NMP must emit an immediate offline diagnostic snapshot")
        XCTAssertEqual(initial?.relays, [])
        XCTAssertEqual(initial?.uncoveredAuthorCount, 0)
        observation = nil

        var replacement: AsyncStream<Pod0NMPDiagnosticsSnapshot>? =
            try await diagnosticsAfterCancellation(from: composition)
        let replacementSnapshot = await firstSnapshot(from: try XCTUnwrap(replacement))
        XCTAssertNotNil(replacementSnapshot)
        replacement = nil

        composition.shutdown()
        let restarted = try Pod0NMPComposition(configuration: configuration, layout: layout)
        let afterRestart = await firstSnapshot(from: try restarted.diagnostics())
        XCTAssertNotNil(afterRestart)
        XCTAssertEqual(afterRestart?.relays, [])
        restarted.shutdown()
        try restarted.resetStoreAfterShutdown()
    }

    private func offlineConfiguration(
        layout: Pod0NMPStoreLayout,
        limits: Pod0NMPConfiguration.Limits = .appDefault
    ) -> Pod0NMPConfiguration {
        Pod0NMPConfiguration(
            storeURL: layout.storeURL,
            indexerRelays: [],
            operatorRelay: nil,
            fallbackRelays: [],
            limits: limits
        )
    }

    private func temporaryRoot(named name: String) -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-nmp-\(name)-\(UUID().uuidString)", isDirectory: true)
    }

    private func firstSnapshot(
        from stream: AsyncStream<Pod0NMPDiagnosticsSnapshot>,
        timeoutSeconds: UInt64 = 5
    ) async -> Pod0NMPDiagnosticsSnapshot? {
        await withTaskGroup(of: Pod0NMPDiagnosticsSnapshot?.self) { group in
            group.addTask {
                for await snapshot in stream {
                    return snapshot
                }
                return nil
            }
            group.addTask {
                try? await Task.sleep(nanoseconds: timeoutSeconds * 1_000_000_000)
                return nil
            }
            let result = await group.next() ?? nil
            group.cancelAll()
            return result
        }
    }

    private func diagnosticsAfterCancellation(
        from composition: Pod0NMPComposition
    ) async throws -> AsyncStream<Pod0NMPDiagnosticsSnapshot> {
        var lastError: Error?
        for _ in 0..<100 {
            do {
                return try composition.diagnostics()
            } catch {
                lastError = error
                try await Task.sleep(nanoseconds: 10_000_000)
            }
        }
        throw lastError ?? Pod0NMPCompositionError.nmpUnavailable
    }
    #endif
}
