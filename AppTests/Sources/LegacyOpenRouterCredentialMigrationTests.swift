import XCTest
@testable import Podcastr

@MainActor
final class LegacyOpenRouterCredentialMigrationTests: XCTestCase {
    func testSaveFailurePreservesLegacySourceWithoutPersisting() throws {
        let fixture = try makeFixture()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fixture.url) }
        var state = fixture.state

        AppStateStore.migrateLegacyOpenRouterSecretIfNeeded(
            in: &state,
            persistence: fixture.persistence,
            saveCredential: { _ in throw InjectedCredentialError.expected },
            readCredential: {
                XCTFail("Read-back must not run after save failure")
                return nil
            }
        )

        XCTAssertEqual(state.settings.legacyOpenRouterAPIKey, fixture.key)
        XCTAssertEqual(fixture.persistence.saveInvocationCount, 0)
        XCTAssertTrue(try fixture.persistedMetadata().contains(fixture.key))
    }

    func testReadBackMismatchPreservesLegacySourceWithoutPersisting() throws {
        let fixture = try makeFixture()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fixture.url) }
        var state = fixture.state

        AppStateStore.migrateLegacyOpenRouterSecretIfNeeded(
            in: &state,
            persistence: fixture.persistence,
            saveCredential: { _ in },
            readCredential: { "different-key" }
        )

        XCTAssertEqual(state.settings.legacyOpenRouterAPIKey, fixture.key)
        XCTAssertEqual(fixture.persistence.saveInvocationCount, 0)
        XCTAssertTrue(try fixture.persistedMetadata().contains(fixture.key))
    }

    func testVerifiedReadBackClearsPlaintextAndPersistsMetadata() throws {
        let fixture = try makeFixture()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fixture.url) }
        var state = fixture.state
        var destination: String?

        AppStateStore.migrateLegacyOpenRouterSecretIfNeeded(
            in: &state,
            persistence: fixture.persistence,
            saveCredential: { destination = $0 },
            readCredential: { destination }
        )

        XCTAssertNil(state.settings.legacyOpenRouterAPIKey)
        XCTAssertEqual(state.settings.openRouterCredentialSource, .manual)
        XCTAssertEqual(fixture.persistence.saveInvocationCount, 1)
        let persisted = try fixture.persistence.load()
        XCTAssertNil(persisted.settings.legacyOpenRouterAPIKey)
        XCTAssertEqual(persisted.settings.openRouterCredentialSource, .manual)
        XCTAssertFalse(try fixture.persistedMetadata().contains(fixture.key))
    }

    private func makeFixture() throws -> (
        url: URL,
        persistence: Persistence,
        state: AppState,
        key: String,
        persistedMetadata: () throws -> String
    ) {
        let url = AppStateTestSupport.uniqueTempFileURL()
        let key = "legacy-openrouter-secret"
        let persistence = Persistence(fileURL: url)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        let encoded = try encoder.encode(AppState())
        var root = try XCTUnwrap(
            JSONSerialization.jsonObject(with: encoded) as? [String: Any]
        )
        var settings = try XCTUnwrap(root["settings"] as? [String: Any])
        settings["openRouterAPIKey"] = key
        root["settings"] = settings
        let legacyMetadata = try JSONSerialization.data(withJSONObject: root)
        try persistence.episodeStore.commitMetadata(legacyMetadata, generation: 1)
        let state = try persistence.load()
        persistence.resetSaveInvocationCount()
        return (
            url,
            persistence,
            state,
            key,
            {
                let data = try XCTUnwrap(persistence.episodeStore.loadMetadata())
                return try XCTUnwrap(String(data: data, encoding: .utf8))
            }
        )
    }
}

private enum InjectedCredentialError: Error {
    case expected
}
