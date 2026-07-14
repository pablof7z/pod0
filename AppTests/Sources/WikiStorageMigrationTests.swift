import XCTest
@testable import Podcastr

// MARK: - WikiStorageMigrationTests

/// Verifies that `WikiPage` and the inventory shape decode old on-disk JSON
/// (written before `schemaVersion` and before any future optional fields)
/// without throwing — and that pages written under a *newer* schema version
/// than this build is aware of are skipped cleanly by `WikiStorage.read`
/// rather than silently parsed against an incompatible shape.
final class WikiStorageMigrationTests: XCTestCase {

    // MARK: - Helpers

    private func tempStorage() -> (storage: WikiStorage, root: URL) {
        let root = FileManager.default
            .temporaryDirectory
            .appendingPathComponent("wiki-storage-migration-\(UUID().uuidString)")
        return (WikiStorage(root: root), root)
    }

    private func write(_ data: Data, to url: URL) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try data.write(to: url, options: .atomic)
    }

    // MARK: - Page decode

    /// An on-disk page written by the *previous* build — no `schemaVersion`
    /// field, no `compileRevision` field, sparse `summary` — must still
    /// decode and default `schemaVersion` to 1.
    func testDecodeOlderPageWithoutSchemaVersion() throws {
        let json = """
        {
          "id": "\(UUID().uuidString)",
          "slug": "ozempic",
          "title": "Ozempic",
          "kind": "topic",
          "scope": { "global": {} },
          "summary": "",
          "sections": [],
          "citations": [],
          "confidence": 0.5,
          "generatedAt": "2025-01-01T00:00:00Z",
          "model": "openai/gpt-4o"
        }
        """.data(using: .utf8)!

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let page = try decoder.decode(WikiPage.self, from: json)

        XCTAssertEqual(page.schemaVersion, 1)
        XCTAssertEqual(page.compileRevision, 1)
        XCTAssertEqual(page.slug, "ozempic")
    }

    /// A page with extra unknown fields (a forward-rolled build wrote it)
    /// but with `schemaVersion: 1` should still decode successfully — JSON
    /// decoders ignore unknown keys by default, and our `decodeIfPresent`
    /// inits don't choke on missing-but-expected keys either.
    func testDecodeIgnoresUnknownFields() throws {
        let json = """
        {
          "id": "\(UUID().uuidString)",
          "slug": "keto",
          "title": "Keto",
          "kind": "topic",
          "scope": { "global": {} },
          "summary": "",
          "sections": [],
          "citations": [],
          "confidence": 0.5,
          "generatedAt": "2025-01-01T00:00:00Z",
          "model": "openai/gpt-4o",
          "schemaVersion": 1,
          "futureFieldNotYetImplemented": "ignored"
        }
        """.data(using: .utf8)!
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        XCTAssertNoThrow(try decoder.decode(WikiPage.self, from: json))
    }

    // MARK: - Storage round-trip + skip

    /// Storage should round-trip a fresh page and stamp the current schema
    /// version on disk.
    func testStorageRoundTripStampsCurrentVersion() throws {
        let (storage, root) = tempStorage()
        defer { try? FileManager.default.removeItem(at: root) }

        let original = WikiPage(
            slug: "ozempic",
            title: "Ozempic",
            kind: .topic,
            scope: .global,
            summary: "Test page"
        )
        try storage.write(original)

        let loaded = try storage.read(slug: original.slug, scope: original.scope)
        XCTAssertNotNil(loaded)
        XCTAssertEqual(loaded?.schemaVersion, WikiPage.currentSchemaVersion)
    }

    /// A page on disk whose `schemaVersion` is greater than this build's
    /// `currentSchemaVersion` must be skipped — never parsed against the
    /// wrong shape.
    func testStorageSkipsNewerSchemaVersion() throws {
        let (storage, root) = tempStorage()
        defer { try? FileManager.default.removeItem(at: root) }

        let newerVersion = WikiPage.currentSchemaVersion + 1
        let pageURL = storage.pageURL(slug: "ozempic", scope: .global)
        let json = """
        {
          "id": "\(UUID().uuidString)",
          "slug": "ozempic",
          "title": "Ozempic",
          "kind": "topic",
          "scope": { "global": {} },
          "summary": "",
          "sections": [],
          "citations": [],
          "confidence": 0.5,
          "generatedAt": "2025-01-01T00:00:00Z",
          "model": "openai/gpt-4o",
          "schemaVersion": \(newerVersion)
        }
        """.data(using: .utf8)!
        try write(json, to: pageURL)

        let loaded = try storage.read(slug: "ozempic", scope: .global)
        XCTAssertNil(loaded, "Storage must refuse pages written under a newer schema version")
    }
}
