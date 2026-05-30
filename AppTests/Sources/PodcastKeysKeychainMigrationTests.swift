import XCTest
@testable import Podcastr

/// Coverage for the M6 per-podcast NIP-F4 secret → Keychain migration.
///
/// The critical correctness risk is **wire-format compatibility**: Swift must
/// parse the exact serde JSON Rust writes for `podcast-keys.json`
/// (`{"schema_version":1,"keys":[{"podcast_id":…,"secret_hex":…}]}`). The
/// parse + batch tests use an injected `save` closure so they run in any
/// harness; one end-to-end test exercises the real Keychain via
/// `PcstIdentityCapability.direct` (the app test host carries the
/// entitlements).
final class PodcastKeysKeychainMigrationTests: XCTestCase {

    private let sampleHex = String(repeating: "ab", count: 32) // 64-char lowercase hex
    private let otherHex = String(repeating: "cd", count: 32)

    private func writeKeysFile(_ json: String) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let file = dir.appendingPathComponent(PodcastKeysKeychainMigration.fileName)
        try json.data(using: .utf8)!.write(to: file)
        return dir
    }

    // MARK: - Pure parse (Rust wire-format compatibility)

    func testParsesRustSerdeWireShape() {
        let json = """
        {"schema_version":1,"keys":[{"podcast_id":"pod-1","secret_hex":"\(sampleHex)"}]}
        """
        let rows = PodcastKeysKeychainMigration.parse(Data(json.utf8))
        XCTAssertEqual(rows.count, 1)
        XCTAssertEqual(rows.first?.podcastID, "pod-1")
        XCTAssertEqual(rows.first?.secretHex, sampleHex)
    }

    func testParsesMultipleKeys() {
        let json = """
        {"schema_version":1,"keys":[\
        {"podcast_id":"pod-a","secret_hex":"\(sampleHex)"},\
        {"podcast_id":"pod-b","secret_hex":"\(otherHex)"}]}
        """
        let rows = PodcastKeysKeychainMigration.parse(Data(json.utf8))
        XCTAssertEqual(rows.count, 2)
        XCTAssertEqual(Set(rows.map(\.podcastID)), ["pod-a", "pod-b"])
    }

    func testRejectsUnknownSchemaVersion() {
        let json = """
        {"schema_version":99,"keys":[{"podcast_id":"pod-1","secret_hex":"\(sampleHex)"}]}
        """
        XCTAssertTrue(PodcastKeysKeychainMigration.parse(Data(json.utf8)).isEmpty)
    }

    func testRejectsMalformedJSON() {
        XCTAssertTrue(PodcastKeysKeychainMigration.parse(Data("not json".utf8)).isEmpty)
    }

    func testSkipsRowWithMalformedSecretHex() {
        let json = """
        {"schema_version":1,"keys":[\
        {"podcast_id":"good","secret_hex":"\(sampleHex)"},\
        {"podcast_id":"bad","secret_hex":"tooshort"}]}
        """
        let rows = PodcastKeysKeychainMigration.parse(Data(json.utf8))
        XCTAssertEqual(rows.map(\.podcastID), ["good"])
    }

    func testHexValidator() {
        XCTAssertTrue(PodcastKeysKeychainMigration.isValidSecretHex(sampleHex))
        XCTAssertFalse(PodcastKeysKeychainMigration.isValidSecretHex("short"))
        XCTAssertFalse(PodcastKeysKeychainMigration.isValidSecretHex(String(repeating: "AB", count: 32)), "uppercase rejected")
        XCTAssertFalse(PodcastKeysKeychainMigration.isValidSecretHex(String(repeating: "zz", count: 32)), "non-hex rejected")
    }

    // MARK: - Account-id convention (must match the M7 Rust read path)

    func testAccountIDConvention() {
        XCTAssertEqual(
            PodcastKeysKeychainMigration.accountID(forPodcastID: "11111111-2222-3333-4444-555555555555"),
            "pcst.podcast.11111111-2222-3333-4444-555555555555.nipf4"
        )
    }

    // MARK: - Batch behaviour (injected save — no Keychain)

    func testMigratesEveryRowToCorrectAccountID() throws {
        let dir = try writeKeysFile("""
        {"schema_version":1,"keys":[\
        {"podcast_id":"pod-a","secret_hex":"\(sampleHex)"},\
        {"podcast_id":"pod-b","secret_hex":"\(otherHex)"}]}
        """)
        var captured: [String: String] = [:]
        let count = PodcastKeysKeychainMigration.runIfNeeded(dataDir: dir) { hex, account in
            captured[account] = hex
        }
        XCTAssertEqual(count, 2)
        XCTAssertEqual(captured["pcst.podcast.pod-a.nipf4"], sampleHex)
        XCTAssertEqual(captured["pcst.podcast.pod-b.nipf4"], otherHex)
    }

    func testMissingFileIsNoOp() {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        var called = false
        let count = PodcastKeysKeychainMigration.runIfNeeded(dataDir: dir) { _, _ in called = true }
        XCTAssertEqual(count, 0)
        XCTAssertFalse(called)
    }

    func testIsIdempotentAcrossRuns() throws {
        let dir = try writeKeysFile("""
        {"schema_version":1,"keys":[{"podcast_id":"pod-1","secret_hex":"\(sampleHex)"}]}
        """)
        var writes = 0
        let saver: (String, String) throws -> Void = { _, _ in writes += 1 }
        _ = PodcastKeysKeychainMigration.runIfNeeded(dataDir: dir, save: saver)
        _ = PodcastKeysKeychainMigration.runIfNeeded(dataDir: dir, save: saver)
        // Both runs upsert (the Keychain store overwrites) — idempotent, not a
        // one-shot guarded by a sentinel.
        XCTAssertEqual(writes, 2)
    }

    func testContinuesPastSaveFailure() throws {
        let dir = try writeKeysFile("""
        {"schema_version":1,"keys":[\
        {"podcast_id":"fails","secret_hex":"\(sampleHex)"},\
        {"podcast_id":"ok","secret_hex":"\(otherHex)"}]}
        """)
        struct StoreError: Error {}
        var stored: [String] = []
        let count = PodcastKeysKeychainMigration.runIfNeeded(dataDir: dir) { _, account in
            if account.contains("fails") { throw StoreError() }
            stored.append(account)
        }
        XCTAssertEqual(count, 1)
        XCTAssertEqual(stored, ["pcst.podcast.ok.nipf4"])
    }

    // MARK: - End-to-end through the real Keychain

    func testRoundTripsThroughRealKeychain() throws {
        let podcastID = "e2e-\(UUID().uuidString)"
        let account = PodcastKeysKeychainMigration.accountID(forPodcastID: podcastID)
        addTeardownBlock { try? PcstIdentityCapability.direct.deleteSecret(for: account) }

        let dir = try writeKeysFile("""
        {"schema_version":1,"keys":[{"podcast_id":"\(podcastID)","secret_hex":"\(sampleHex)"}]}
        """)
        let count = PodcastKeysKeychainMigration.runIfNeeded(dataDir: dir)
        XCTAssertEqual(count, 1)
        XCTAssertEqual(try PcstIdentityCapability.direct.loadSecret(for: account), sampleHex)
    }
}
