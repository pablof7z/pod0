import XCTest
@testable import Podcastr

/// Coverage for `OPMLExport.exportOPML`. Pairs with `OPMLImportTests`:
/// the most valuable assertion here is that **export → re-import → same
/// subscriptions** — guarantees the two halves of the OPML pipeline stay
/// in sync, which is what makes the "Export OPML" feature actually
/// portable to other podcast apps.
@MainActor
final class OPMLExportTests: XCTestCase {

    // MARK: - Output shape

    func testProducesOPML2Envelope() throws {
        let data = OPMLExport().exportOPML(podcasts: [], title: "Mine")
        let xml = try XCTUnwrap(String(data: data, encoding: .utf8))

        XCTAssertTrue(xml.hasPrefix("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"))
        XCTAssertTrue(xml.contains("<opml version=\"2.0\">"))
        XCTAssertTrue(xml.contains("<title>Mine</title>"))
        XCTAssertTrue(xml.contains("<body>"))
        XCTAssertTrue(xml.contains("</opml>"))
    }

    func testEmitsRequiredOutlineAttributesPerSubscription() throws {
        let sub = Podcast(
            feedURL: URL(string: "https://example.com/feed.xml")!,
            title: "Example Show"
        )

        let xml = exportXML([sub])

        XCTAssertTrue(xml.contains("type=\"rss\""))
        XCTAssertTrue(xml.contains("xmlUrl=\"https://example.com/feed.xml\""))
        XCTAssertTrue(xml.contains("text=\"Example Show\""))
        XCTAssertTrue(xml.contains("title=\"Example Show\""))
    }

    // MARK: - Round-trip with the importer

    func testExportThenImportPreservesSubscriptions() throws {
        let originals: [Podcast] = [
            Podcast(
                feedURL: URL(string: "https://example.com/tim.xml")!,
                title: "Tim Ferriss",
                description: "Long-form interviews",
                language: "en"
            ),
            Podcast(
                feedURL: URL(string: "https://example.com/acq.xml")!,
                title: "Acquired",
                description: "Tech company histories",
                language: "en"
            ),
        ]

        let data = OPMLExport().exportOPML(podcasts: originals)
        let imported = try OPMLImport().parseOPML(data: data)

        XCTAssertEqual(imported.count, 2)
        XCTAssertEqual(imported.map(\.feedURL), originals.map(\.feedURL))
        XCTAssertEqual(imported.map(\.title), originals.map(\.title))
        XCTAssertEqual(imported.map(\.description), originals.map(\.description))
        XCTAssertEqual(imported.map(\.language), originals.map(\.language))
    }

    // MARK: - Escaping

    func testEscapesXMLPredefinedEntitiesInAttributes() throws {
        let sub = Podcast(
            feedURL: URL(string: "https://example.com/feed.xml?a=1&b=2")!,
            title: "AT&T <Tech>",
            description: "Quotes \" and ' apostrophes"
        )

        let xml = exportXML([sub])

        // Title with `&` and `<>` must be escaped to entities.
        XCTAssertTrue(xml.contains("text=\"AT&amp;T &lt;Tech&gt;\""))
        XCTAssertTrue(xml.contains("title=\"AT&amp;T &lt;Tech&gt;\""))
        // Description with quotes / apostrophes must be escaped.
        XCTAssertTrue(xml.contains("description=\"Quotes &quot; and &apos; apostrophes\""))
        // Feed URL `&` must also be escaped.
        XCTAssertTrue(xml.contains("xmlUrl=\"https://example.com/feed.xml?a=1&amp;b=2\""))
    }

    func testFoldsNewlinesInsideAttributesToSpaces() throws {
        let sub = Podcast(
            feedURL: URL(string: "https://example.com/feed.xml")!,
            title: "Show",
            description: "Line 1\nLine 2\rLine 3"
        )

        let xml = exportXML([sub])

        // Newlines within an attribute would break a one-line outline; the
        // exporter folds them to spaces.
        XCTAssertTrue(xml.contains("description=\"Line 1 Line 2 Line 3\""))
        XCTAssertFalse(xml.contains("Line 1\nLine 2"))
    }

    // MARK: - Conditional fields

    func testOmitsEmptyDescriptionAndLanguage() throws {
        let sub = Podcast(
            feedURL: URL(string: "https://example.com/feed.xml")!,
            title: "Bare Show"
        )

        let xml = exportXML([sub])

        XCTAssertFalse(xml.contains("description="))
        XCTAssertFalse(xml.contains("language="))
    }

    // MARK: - Header

    func testIncludesDateCreatedHeader() throws {
        // RFC 822 format includes a four-letter weekday + full month name.
        let date = Date(timeIntervalSince1970: 1_700_000_000) // 2023-11-14T22:13:20Z
        let data = OPMLExport().exportOPML(podcasts: [], dateCreated: date)
        let xml = try XCTUnwrap(String(data: data, encoding: .utf8))

        XCTAssertTrue(xml.contains("<dateCreated>"))
        XCTAssertTrue(xml.contains("2023"))
    }

    // MARK: - Helpers

    private func exportXML(_ subs: [Podcast]) -> String {
        let data = OPMLExport().exportOPML(podcasts: subs)
        return String(data: data, encoding: .utf8) ?? ""
    }
}
