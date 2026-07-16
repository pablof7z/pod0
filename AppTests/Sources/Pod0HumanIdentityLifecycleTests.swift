import Foundation
import NMP
import XCTest
@testable import Podcastr

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
        let localEntry = try await firstLifecycle.registerLocal(
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

    func testEmptySignOutIsIdempotent() throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        let fixture = try makeFixture(checkpoint: checkpoint)
        defer { fixture.dispose() }
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: fixture.composition,
            catalogStorage: catalog
        )

        try lifecycle.cachePreservingSignOut()
        try lifecycle.cachePreservingSignOut()

        XCTAssertEqual(lifecycle.state, .signedOut)
        XCTAssertNil(lifecycle.blocker)
        XCTAssertNil(try fixture.composition.engine.activeAccount())
        XCTAssertNil(try catalog.load())
    }

    func testMissingLocalCheckpointCanClearStaleCatalog() async throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        let fixture = try makeFixture(checkpoint: checkpoint)
        defer { fixture.dispose() }
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: fixture.composition,
            catalogStorage: catalog
        )
        try catalog.save(catalogContaining(Pod0IdentityCatalogEntry(
            role: .human,
            label: "Missing local",
            origin: .importedNsec,
            expectedPublicKey: try NostrKeyPair.generate().publicKeyHex,
            capability: .localKey(secretReference: Pod0HumanIdentityLifecycle.localSecretReference),
            createdAt: Date()
        )))

        do {
            _ = try await lifecycle.restoreHuman()
            XCTFail("Expected the missing checkpoint to reject restore")
        } catch {
            XCTAssertEqual(error as? Pod0HumanIdentityError, .missingRestoredLocalAccount)
        }

        try lifecycle.cachePreservingSignOut()
        XCTAssertNil(try catalog.load())
        XCTAssertEqual(lifecycle.state, .signedOut)
        XCTAssertNil(lifecycle.blocker)
    }

    func testFailedBunkerRestoreCanClearStaleCatalog() async throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        let fixture = try makeFixture(checkpoint: checkpoint)
        defer { fixture.dispose() }
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: fixture.composition,
            catalogStorage: catalog
        )
        try catalog.save(catalogContaining(Pod0IdentityCatalogEntry(
            role: .human,
            label: "Broken bunker",
            origin: .bunker,
            expectedPublicKey: String(repeating: "b", count: 64),
            capability: .nip46Bunker(uri: "not-a-bunker-uri"),
            createdAt: Date()
        )))

        do {
            _ = try await lifecycle.restoreHuman()
            XCTFail("Expected invalid bunker restore to fail")
        } catch {
            XCTAssertNil(lifecycle.blocker)
        }

        try lifecycle.cachePreservingSignOut()
        XCTAssertNil(try catalog.load())
        XCTAssertEqual(lifecycle.state, .signedOut)
    }

    func testBunkerCatalogRejectsAndProtectsRestoredLocalCheckpoint() async throws {
        let checkpoint = TestLocalAccountCheckpoint()
        let catalog = TestIdentityCatalogStorage()
        let root = temporaryRoot("bunker-local-mismatch")
        let first = try makeFixture(root: root, checkpoint: checkpoint)
        let firstLifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: first.composition,
            catalogStorage: catalog
        )
        let localEntry = try await firstLifecycle.registerLocal(
            secret: try NostrKeyPair.generate().nsec,
            origin: .importedNsec,
            label: "Local"
        )
        let bunkerEntry = Pod0IdentityCatalogEntry(
            role: .human,
            label: "Bunker",
            origin: .bunker,
            expectedPublicKey: String(repeating: "c", count: 64),
            capability: .nip46Bunker(uri: "not-a-bunker-uri"),
            createdAt: Date()
        )
        try catalog.save(catalogContaining(bunkerEntry))
        first.composition.shutdown()

        let restored = try makeFixture(root: root, checkpoint: checkpoint)
        defer { restored.dispose() }
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: restored.composition,
            catalogStorage: catalog
        )
        do {
            _ = try await lifecycle.restoreHuman()
            XCTFail("Expected restored local signer to block bunker restore")
        } catch {
            XCTAssertEqual(lifecycle.blocker, .orphanedRestoredLocal(issue: 589))
        }
        XCTAssertNil(try restored.composition.engine.activeAccount())
        XCTAssertEqual(lifecycle.restoredLocalPublicKey, localEntry.expectedPublicKey)
        XCTAssertNotNil(checkpoint.storedSecret)
        XCTAssertEqual(try catalog.load()?.entry(for: .human), bunkerEntry)

        do {
            _ = try await lifecycle.restoreHuman()
            XCTFail("Expected the orphan blocker to remain sticky")
        } catch {
            XCTAssertEqual(lifecycle.blocker, .orphanedRestoredLocal(issue: 589))
        }
        XCTAssertNil(try restored.composition.engine.activeAccount())
        XCTAssertEqual(lifecycle.restoredLocalPublicKey, localEntry.expectedPublicKey)
        XCTAssertEqual(try catalog.load()?.entry(for: .human), bunkerEntry)
        XCTAssertNotNil(checkpoint.storedSecret)

        XCTAssertThrowsError(try lifecycle.cachePreservingSignOut())
        XCTAssertEqual(try catalog.load()?.entry(for: .human), bunkerEntry)
        XCTAssertNotNil(checkpoint.storedSecret)
    }
}
