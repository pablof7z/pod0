import Foundation
import NMP
import XCTest
@testable import Podcastr

final class Pod0IdentityCatalogTests: XCTestCase {
    func testCatalogRoundTripsMultipleRolesWithoutSecretMaterial() throws {
        let human = Pod0IdentityCatalogEntry(
            role: .human,
            label: "Alice",
            origin: .importedNsec,
            expectedPublicKey: String(repeating: "a", count: 64),
            capability: .localKey(secretReference: "human-account-key"),
            createdAt: Date(timeIntervalSince1970: 100)
        )
        let agent = Pod0IdentityCatalogEntry(
            role: .agentPodcast,
            label: "Podcast agent",
            origin: .generatedLocally,
            expectedPublicKey: String(repeating: "b", count: 64),
            capability: .reservedForLaterMilestone,
            createdAt: Date(timeIntervalSince1970: 200)
        )
        var catalog = Pod0IdentityCatalog(selectedRole: .human)
        catalog.upsert(human)
        catalog.upsert(agent)

        let data = try JSONEncoder().encode(catalog)
        let decoded = try JSONDecoder().decode(Pod0IdentityCatalog.self, from: data)

        XCTAssertEqual(decoded, catalog)
        XCTAssertEqual(decoded.entry(for: .human)?.expectedPublicKey, human.expectedPublicKey)
        XCTAssertEqual(decoded.entry(for: .agentPodcast)?.expectedPublicKey, agent.expectedPublicKey)
        XCTAssertFalse(String(decoding: data, as: UTF8.self).contains("nsec1"))
    }

    func testUnknownRoleCannotBeSelected() {
        var catalog = Pod0IdentityCatalog()
        XCTAssertThrowsError(try catalog.select(.agentPodcast)) { error in
            XCTAssertEqual(error as? Pod0IdentityCatalogError, .roleNotFound(.agentPodcast))
        }
    }

    func testClientInitiatedCheckpointBlockerNamesUpstreamGate() throws {
        let blocker = Pod0IdentityBlocker.clientInitiatedNip46CheckpointUnsupported(issue: 571)
        let data = try JSONEncoder().encode(blocker)
        let decoded = try JSONDecoder().decode(Pod0IdentityBlocker.self, from: data)
        XCTAssertEqual(decoded, blocker)
    }

    func testRestoredLocalDetachBlockerNamesUpstreamGate() throws {
        let blocker = Pod0IdentityBlocker.restoredLocalDetachUnsupported(issue: 589)
        let data = try JSONEncoder().encode(blocker)
        XCTAssertEqual(try JSONDecoder().decode(Pod0IdentityBlocker.self, from: data), blocker)
    }

    func testCleanStartHumanSecretStoreRoundTripsOnlyRequestedReference() throws {
        let store = NMPKeychainAccountStore(
            service: "pod0-tests.nmp-human-identity.\(UUID().uuidString)",
            account: Pod0HumanIdentityLifecycle.localSecretReference
        )
        defer { try? store.clear() }

        XCTAssertNil(try store.loadSecretKey())
        try store.saveSecretKey("test-secret")
        XCTAssertEqual(try store.loadSecretKey(), "test-secret")
        try store.clear()
        XCTAssertNil(try store.loadSecretKey())
    }
}

@MainActor
final class Pod0HumanIdentityLifecycleTests: XCTestCase {
    func testSameProcessLocalAccountDetachesExactly() async throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        let fixture = try makeFixture(checkpoint: checkpoint)
        defer { fixture.dispose() }
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: fixture.composition,
            catalogStorage: catalog
        )
        let key = try NostrKeyPair.generate()

        let entry = try await lifecycle.registerLocal(
            secret: key.nsec,
            origin: .importedNsec,
            label: "Test"
        )
        XCTAssertEqual(try fixture.composition.engine.activeAccount(), entry.expectedPublicKey)
        XCTAssertNotNil(checkpoint.storedSecret)

        try lifecycle.cachePreservingSignOut()

        XCTAssertNil(try fixture.composition.engine.activeAccount())
        XCTAssertNil(try catalog.load())
        XCTAssertNil(checkpoint.storedSecret)
        XCTAssertEqual(lifecycle.state, .signedOut)
    }

    func testColdRestoredLocalAccountRefusesDetachWithoutExactHandle() async throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        let root = temporaryRoot("cold-restore")
        let first = try makeFixture(root: root, checkpoint: checkpoint)
        let lifecycle = Pod0HumanIdentityLifecycle(engineAccess: first.composition, catalogStorage: catalog)
        let key = try NostrKeyPair.generate()
        let entry = try await lifecycle.registerLocal(
            secret: key.nsec,
            origin: .importedNsec,
            label: "Test"
        )
        first.composition.shutdown()

        let restored = try makeFixture(root: root, checkpoint: checkpoint)
        defer { restored.dispose() }
        let coldLifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: restored.composition,
            catalogStorage: catalog
        )
        _ = try await coldLifecycle.restoreHuman()

        XCTAssertThrowsError(try coldLifecycle.cachePreservingSignOut()) { error in
            XCTAssertEqual(
                error as? Pod0HumanIdentityError,
                .restoredLocalDetachUnsupported(issue: 589)
            )
        }
        XCTAssertEqual(try restored.composition.engine.activeAccount(), entry.expectedPublicKey)
        XCTAssertNotNil(try catalog.load())
        XCTAssertNotNil(checkpoint.storedSecret)
    }

    func testCataloglessRestoredAccountBlocksReplacementImport() async throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        let root = temporaryRoot("orphan")
        let first = try makeFixture(root: root, checkpoint: checkpoint)
        let firstLifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: first.composition,
            catalogStorage: catalog
        )
        let key = try NostrKeyPair.generate()
        _ = try await firstLifecycle.registerLocal(
            secret: key.nsec,
            origin: .importedNsec,
            label: "Test"
        )
        try catalog.clear()
        first.composition.shutdown()

        let restored = try makeFixture(root: root, checkpoint: checkpoint)
        defer { restored.dispose() }
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: restored.composition,
            catalogStorage: catalog
        )
        do {
            _ = try await lifecycle.restoreHuman()
            XCTFail("Expected the catalog-less restored account to block")
        } catch {
            XCTAssertEqual(lifecycle.blocker, .orphanedRestoredLocal(issue: 589))
        }
        XCTAssertNil(try restored.composition.engine.activeAccount())
        XCTAssertNotNil(checkpoint.storedSecret)

        do {
            _ = try await lifecycle.registerLocal(
                secret: try NostrKeyPair.generate().nsec,
                origin: .importedNsec,
                label: "Replacement"
            )
            XCTFail("Expected replacement import to remain blocked")
        } catch {
            XCTAssertEqual(error as? Pod0HumanIdentityError, .identitySwitchUnsupported)
        }
    }

    func testCheckpointClearFailureUsesExplicitRecoveryWithoutReactivation() async throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        let fixture = try makeFixture(checkpoint: checkpoint)
        defer { fixture.dispose() }
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: fixture.composition,
            catalogStorage: catalog
        )
        let key = try NostrKeyPair.generate()
        _ = try await lifecycle.registerLocal(
            secret: key.nsec,
            origin: .importedNsec,
            label: "Test"
        )
        checkpoint.failNextClear()

        try lifecycle.cachePreservingSignOut()

        XCTAssertNil(try fixture.composition.engine.activeAccount())
        XCTAssertNil(checkpoint.storedSecret)
        XCTAssertNil(try catalog.load())
        XCTAssertEqual(checkpoint.clearAttemptCount, 2)
    }

    func testCatalogSaveFailureRollsBackActiveAccountAndCheckpoint() async throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        catalog.failNextSave()
        let fixture = try makeFixture(checkpoint: checkpoint)
        defer { fixture.dispose() }
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: fixture.composition,
            catalogStorage: catalog
        )

        do {
            _ = try await lifecycle.registerLocal(
                secret: try NostrKeyPair.generate().nsec,
                origin: .importedNsec,
                label: "Test"
            )
            XCTFail("Expected catalog persistence failure")
        } catch {
            XCTAssertNil(try fixture.composition.engine.activeAccount())
            XCTAssertNil(checkpoint.storedSecret)
            XCTAssertNil(try catalog.load())
        }
    }

    private func makeFixture(
        root: URL? = nil,
        checkpoint: TestLocalAccountCheckpoint
    ) throws -> LifecycleFixture {
        let root = root ?? temporaryRoot("lifecycle")
        let layout = Pod0NMPStoreLayout(rootDirectory: root)
        let configuration = Pod0NMPConfiguration(
            storeURL: layout.storeURL,
            indexerRelays: [],
            operatorRelay: nil,
            fallbackRelays: []
        )
        return LifecycleFixture(
            root: root,
            composition: try Pod0NMPComposition(
                configuration: configuration,
                layout: layout,
                localAccountStore: checkpoint
            )
        )
    }

    private func temporaryRoot(_ name: String) -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-identity-\(name)-\(UUID().uuidString)", isDirectory: true)
    }
}

private struct LifecycleFixture {
    let root: URL
    let composition: Pod0NMPComposition

    func dispose() {
        composition.shutdown()
        try? composition.resetStoreAfterShutdown()
        try? FileManager.default.removeItem(at: root)
    }
}

private enum TestIdentityStorageError: Error {
    case injected
}

private final class TestLocalAccountCheckpoint: NMPLocalAccountCheckpoint, @unchecked Sendable {
    private let lock = NSLock()
    private var secret: String?
    private var pendingClearFailures = 0
    private var clearAttempts = 0

    var storedSecret: String? { lock.withLock { secret } }
    var clearAttemptCount: Int { lock.withLock { clearAttempts } }

    func loadSecretKey() throws -> String? { lock.withLock { secret } }

    func saveSecretKey(_ secretKey: String) throws {
        lock.withLock { secret = secretKey }
    }

    func clear() throws {
        try lock.withLock {
            clearAttempts += 1
            if pendingClearFailures > 0 {
                pendingClearFailures -= 1
                throw TestIdentityStorageError.injected
            }
            secret = nil
        }
    }

    func failNextClear() {
        lock.withLock { pendingClearFailures += 1 }
    }
}

private final class TestIdentityCatalogStorage: Pod0IdentityCatalogStorage, @unchecked Sendable {
    private let lock = NSLock()
    private var catalog: Pod0IdentityCatalog?
    private var pendingSaveFailures = 0

    func load() throws -> Pod0IdentityCatalog? { lock.withLock { catalog } }

    func save(_ catalog: Pod0IdentityCatalog) throws {
        try lock.withLock {
            if pendingSaveFailures > 0 {
                pendingSaveFailures -= 1
                throw TestIdentityStorageError.injected
            }
            self.catalog = catalog
        }
    }

    func clear() throws {
        lock.withLock { catalog = nil }
    }

    func failNextSave() {
        lock.withLock { pendingSaveFailures += 1 }
    }
}
