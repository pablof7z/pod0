import XCTest
@testable import Podcastr

/// Coverage for `OPMLImport.parseOPML`. The OPML import path runs at
/// onboarding and via Library → Add Show → OPML, so it deserves
/// regression tests on the parsing rules: title fallback chain,
/// duplicate-feed-URL de-dup, grouping folders skipped, malformed XML
/// surfaced cleanly, etc.
@MainActor
final class OPMLImportTests: XCTestCase {

    // MARK: - Happy path

    func testParsesBasicTwoShowOPML() throws {
        let data = makeOPML(body: """
        <outline text="Tim Ferriss" type="rss" xmlUrl="https://example.com/tim.xml"/>
        <outline text="Acquired"     type="rss" xmlUrl="https://example.com/acq.xml"/>
        """)

        let subs = try OPMLImport().parseOPML(data: data)

        XCTAssertEqual(subs.count, 2)
        XCTAssertEqual(subs[0].title, "Tim Ferriss")
        XCTAssertEqual(subs[0].feedURL, URL(string: "https://example.com/tim.xml")!)
        XCTAssertEqual(subs[1].title, "Acquired")
        XCTAssertEqual(subs[1].feedURL, URL(string: "https://example.com/acq.xml")!)
    }

    // MARK: - Title fallback chain

    func testTitleFallsBackToTitleAttribute() throws {
        // No `text`, but `title` is present.
        let data = makeOPML(body: """
        <outline title="Lex Fridman" type="rss" xmlUrl="https://example.com/lex.xml"/>
        """)

        let subs = try OPMLImport().parseOPML(data: data)

        XCTAssertEqual(subs.count, 1)
        XCTAssertEqual(subs[0].title, "Lex Fridman")
    }

    func testTitleFallsBackToHostWhenAttributesMissing() throws {
        // No `text`, no `title` — should fall back to the feed URL host.
        let data = makeOPML(body: """
        <outline type="rss" xmlUrl="https://feeds.example.com/abc.xml"/>
        """)

        let subs = try OPMLImport().parseOPML(data: data)

        XCTAssertEqual(subs.count, 1)
        XCTAssertEqual(subs[0].title, "feeds.example.com")
    }

    // MARK: - De-dup + skipping

    func testDeduplicatesRepeatedFeedURLs() throws {
        let data = makeOPML(body: """
        <outline text="First"  type="rss" xmlUrl="https://example.com/feed.xml"/>
        <outline text="Second" type="rss" xmlUrl="https://example.com/feed.xml"/>
        """)

        let subs = try OPMLImport().parseOPML(data: data)

        XCTAssertEqual(subs.count, 1)
        XCTAssertEqual(subs[0].title, "First", "First-seen wins on duplicates")
    }

    func testSkipsOutlinesWithoutXmlUrl() throws {
        // First outline is a grouping folder (no `xmlUrl`); second is a real feed.
        let data = makeOPML(body: """
        <outline text="Tech" title="Tech">
            <outline text="ATP" type="rss" xmlUrl="https://example.com/atp.xml"/>
        </outline>
        """)

        let subs = try OPMLImport().parseOPML(data: data)

        XCTAssertEqual(subs.count, 1)
        XCTAssertEqual(subs[0].title, "ATP")
    }

    // MARK: - Captured metadata

    func testCapturesDescriptionAndLanguage() throws {
        let data = makeOPML(body: """
        <outline text="Show" type="rss"
                 xmlUrl="https://example.com/show.xml"
                 description="A great show about things"
                 language="en"/>
        """)

        let subs = try OPMLImport().parseOPML(data: data)

        XCTAssertEqual(subs.count, 1)
        XCTAssertEqual(subs[0].description, "A great show about things")
        XCTAssertEqual(subs[0].language, "en")
    }

    // MARK: - Error surface

    func testThrowsOnMalformedXML() {
        let data = "<<not actually xml>>".data(using: .utf8)!

        XCTAssertThrowsError(try OPMLImport().parseOPML(data: data)) { error in
            guard case OPMLImport.OPMLError.malformedXML = error else {
                XCTFail("Expected .malformedXML, got \(error)")
                return
            }
        }
    }

    // MARK: - Empty cases

    func testReturnsEmptyArrayWhenNoFeeds() throws {
        let data = makeOPML(body: "")
        let subs = try OPMLImport().parseOPML(data: data)
        XCTAssertTrue(subs.isEmpty)
    }

    // MARK: - Helpers

    /// Wraps the supplied body in a minimal valid OPML 2.0 envelope so the
    /// individual tests stay focused on the `<outline>` permutations.
    private func makeOPML(body: String) -> Data {
        let xml = """
        <?xml version="1.0" encoding="UTF-8"?>
        <opml version="2.0">
          <head><title>Subscriptions</title></head>
          <body>
        \(body)
          </body>
        </opml>
        """
        return xml.data(using: .utf8)!
    }
}
