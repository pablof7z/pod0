import Foundation
import NMP
import XCTest
@testable import Podcastr

final class Pod0IdentityCatalogTests: XCTestCase {
    func testCatalogRoundTripsImportedHumanWithoutSecretMaterial() throws {
        let human = Pod0IdentityCatalogEntry(
            role: .human,
            label: "Alice",
            origin: .importedNsec,
            expectedPublicKey: String(repeating: "a", count: 64),
            capability: .localKey(secretReference: "human-account-key"),
            createdAt: Date(timeIntervalSince1970: 100)
        )
        var catalog = Pod0IdentityCatalog(selectedRole: .human)
        catalog.upsert(human)

        let data = try JSONEncoder().encode(catalog)
        let decoded = try JSONDecoder().decode(Pod0IdentityCatalog.self, from: data)

        XCTAssertEqual(decoded, catalog)
        XCTAssertEqual(decoded.entry(for: .human)?.expectedPublicKey, human.expectedPublicKey)
        XCTAssertFalse(String(decoding: data, as: UTF8.self).contains("nsec1"))
    }

    func testMissingEntryCannotBeSelected() {
        var catalog = Pod0IdentityCatalog()
        XCTAssertThrowsError(try catalog.select(.human)) { error in
            XCTAssertEqual(error as? Pod0IdentityCatalogError, .roleNotFound(.human))
        }
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
