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
