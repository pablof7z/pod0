import Foundation
import XCTest
@testable import Podcastr

final class Pod0IdentityCatalogTests: XCTestCase {
    func testCatalogRoundTripsMultipleRolesWithoutSecretMaterial() throws {
        let human = Pod0IdentityCatalogEntry(
            role: .human,
            label: "Alice",
            origin: .importedNsec,
            expectedPublicKey: String(repeating: "a", count: 64),
            capability: .localKey(secretReference: "legacy-human-private-key"),
            createdAt: Date(timeIntervalSince1970: 100)
        )
        let agent = Pod0IdentityCatalogEntry(
            role: .agentPodcast,
            label: "Podcast agent",
            origin: .legacyAgentKey,
            expectedPublicKey: String(repeating: "b", count: 64),
            capability: .reservedForLaterMilestone(secretReference: "legacy-agent-private-key"),
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

    func testClientInitiatedRestoreBlockerNamesUpstreamGate() throws {
        let blocker = Pod0IdentityBlocker.clientInitiatedNip46RestoreUnsupported(issue: 571)
        let data = try JSONEncoder().encode(blocker)
        let decoded = try JSONDecoder().decode(Pod0IdentityBlocker.self, from: data)
        XCTAssertEqual(decoded, blocker)
    }
}

