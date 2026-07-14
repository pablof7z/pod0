import XCTest
@testable import Podcastr

final class RSSParserTests: XCTestCase {

    // MARK: - Fixture

    /// A compact feed exercising base RSS 2.0 + iTunes namespace + Podcasting
    /// 2.0 transcript / chapters / person / soundbite. Two items: one fully
    /// fledged, one minimal-with-no-guid to exercise the synthesized GUID
    /// path.
    private static let fixtureXML: String = #"""
    <?xml version="1.0" encoding="UTF-8"?>
    <rss version="2.0"
         xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
         xmlns:podcast="https://podcastindex.org/namespace/1.0"
         xmlns:content="http://purl.org/rss/1.0/modules/content/">
      <channel>
        <title>Example Show</title>
        <description>A test feed.</description>
        <language>en-US</language>
        <itunes:author>Test Author</itunes:author>
        <itunes:image href="https://example.com/cover.jpg"/>
        <itunes:category text="Technology"/>
        <itunes:category text="News"/>
        <image>
          <url>https://example.com/legacy.jpg</url>
          <title>Example Show</title>
        </image>

        <item>
          <title>Episode 1: Hello</title>
          <description><![CDATA[<p>Show notes for episode 1.</p>]]></description>
          <pubDate>Mon, 04 May 2026 09:00:00 GMT</pubDate>
          <guid isPermaLink="false">ep-0001</guid>
          <itunes:duration>1:23:45</itunes:duration>
          <enclosure url="https://example.com/audio/ep1.mp3"
                     length="12345678" type="audio/mpeg"/>
          <itunes:image href="https://example.com/ep1.jpg"/>
          <podcast:transcript url="https://example.com/ep1.vtt" type="text/vtt"/>
          <podcast:transcript url="https://example.com/ep1.json"
                              type="application/json"/>
          <podcast:chapters url="https://example.com/ep1.chapters.json"
                            type="application/json+chapters"/>
          <podcast:person role="host"
                          href="https://example.com/host"
                          img="https://example.com/host.jpg">Alice Host</podcast:person>
          <podcast:person role="guest">Bob Guest</podcast:person>
          <podcast:soundbite startTime="120.5" duration="30.0">Hot take</podcast:soundbite>
        </item>

        <item>
          <title>Episode 2: No GUID</title>
          <description>Minimal episode.</description>
          <pubDate>Tue, 05 May 2026 09:00:00 GMT</pubDate>
          <enclosure url="https://example.com/audio/ep2.mp3" type="audio/mpeg"/>
        </item>
      </channel>
    </rss>
    """#

    // MARK: - Parsing

    func testParsesChannelMetadata() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )

        XCTAssertEqual(result.podcast.title, "Example Show")
        XCTAssertEqual(result.podcast.author, "Test Author")
        XCTAssertEqual(result.podcast.language, "en-US")
        XCTAssertEqual(result.podcast.description, "A test feed.")
        XCTAssertEqual(result.podcast.imageURL?.absoluteString, "https://example.com/cover.jpg")
        XCTAssertEqual(result.podcast.categories, ["Technology", "News"])
        XCTAssertEqual(result.podcast.feedURL?.absoluteString, "https://example.com/feed.xml")
    }

    func testParsesEpisodeBaseFields() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        XCTAssertEqual(result.episodes.count, 2)

        let ep1 = result.episodes[0]
        XCTAssertEqual(ep1.title, "Episode 1: Hello")
        XCTAssertEqual(ep1.guid, "ep-0001")
        XCTAssertEqual(ep1.enclosureURL.absoluteString, "https://example.com/audio/ep1.mp3")
        XCTAssertEqual(ep1.enclosureMimeType, "audio/mpeg")
        XCTAssertEqual(ep1.duration, 5025) // 1h 23m 45s
        XCTAssertEqual(ep1.imageURL?.absoluteString, "https://example.com/ep1.jpg")
        XCTAssertTrue(ep1.description.contains("Show notes for episode 1"))
    }

    func testParsesPubDateAsRFC822() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        let ep1 = result.episodes[0]

        var components = DateComponents()
        components.year = 2026
        components.month = 5
        components.day = 4
        components.hour = 9
        components.timeZone = TimeZone(identifier: "GMT")
        let expected = Calendar(identifier: .gregorian).date(from: components)!
        XCTAssertEqual(ep1.pubDate.timeIntervalSince1970, expected.timeIntervalSince1970, accuracy: 1.0)
    }

    // MARK: - Podcasting 2.0 coverage

    func testPicksHighestRankTranscript() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        let ep1 = result.episodes[0]
        XCTAssertEqual(ep1.publisherTranscriptType, .json)
        XCTAssertEqual(ep1.publisherTranscriptURL?.absoluteString, "https://example.com/ep1.json")
    }

    func testParsesChaptersURL() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        XCTAssertEqual(
            result.episodes[0].chaptersURL?.absoluteString,
            "https://example.com/ep1.chapters.json"
        )
    }

    func testParsesPersonsAndSoundBites() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        let ep1 = result.episodes[0]

        let persons = ep1.persons ?? []
        XCTAssertEqual(persons.count, 2)
        XCTAssertEqual(persons[0].name, "Alice Host")
        XCTAssertEqual(persons[0].role, "host")
        XCTAssertEqual(persons[0].imageURL?.absoluteString, "https://example.com/host.jpg")
        XCTAssertEqual(persons[1].name, "Bob Guest")
        XCTAssertEqual(persons[1].role, "guest")

        let bites = ep1.soundBites ?? []
        XCTAssertEqual(bites.count, 1)
        XCTAssertEqual(bites[0].startTime, 120.5)
        XCTAssertEqual(bites[0].duration, 30.0)
        XCTAssertEqual(bites[0].title, "Hot take")
    }

    // MARK: - GUID synthesis

    func testSynthesizedGUIDForItemMissingGUID() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        let ep2 = result.episodes[1]
        XCTAssertTrue(ep2.guid.hasPrefix("synth::"))
        // Stable across re-parse.
        let result2 = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!,
            podcastID: result.podcast.id
        )
        XCTAssertEqual(result2.episodes[1].guid, ep2.guid)
    }

    func testSubscriptionIDPropagatesToEpisodes() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let subscriptionID = UUID()
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!,
            podcastID: subscriptionID
        )
        XCTAssertEqual(result.podcast.id, subscriptionID)
        for episode in result.episodes {
            XCTAssertEqual(episode.podcastID, subscriptionID)
        }
    }

    // MARK: - Codable round-trip

    func testEpisodeCodableRoundTrip() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        let original = result.episodes[0]
        let encoded = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Episode.self, from: encoded)
        XCTAssertEqual(decoded, original)
    }

    func testSubscriptionCodableRoundTrip() throws {
        let parser = RSSParser()
        let data = Self.fixtureXML.data(using: .utf8)!
        let result = try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        let original = result.podcast
        let encoded = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Podcast.self, from: encoded)
        XCTAssertEqual(decoded, original)
    }

    // MARK: - OPML round-trip

    func testOPMLExportThenImportRoundTrip() throws {
        let original = [
            Podcast(
                feedURL: URL(string: "https://feeds.example.com/show1.rss")!,
                title: "Show One",
                description: "First show",
                language: "en-US"
            ),
            Podcast(
                feedURL: URL(string: "https://feeds.example.com/show2.rss")!,
                title: "Show Two & More",  // exercise XML escaping
                description: "Second show",
                language: nil
            ),
        ]

        let bytes = OPMLExport().exportOPML(podcasts: original)
        let xml = String(data: bytes, encoding: .utf8) ?? ""
        XCTAssertTrue(xml.contains("<opml version=\"2.0\">"))
        XCTAssertTrue(xml.contains("Show One"))
        XCTAssertTrue(xml.contains("Show Two &amp; More"))

        let imported = try OPMLImport().parseOPML(data: bytes)
        XCTAssertEqual(imported.count, 2)
        XCTAssertEqual(imported[0].feedURL, original[0].feedURL)
        XCTAssertEqual(imported[0].title, "Show One")
        XCTAssertEqual(imported[1].feedURL, original[1].feedURL)
        XCTAssertEqual(imported[1].title, "Show Two & More") // unescaped on import
    }

    func testOPMLImportSkipsOutlinesWithoutXmlUrl() throws {
        let opml = #"""
        <?xml version="1.0" encoding="UTF-8"?>
        <opml version="2.0">
          <body>
            <outline text="Folder">
              <outline type="rss" text="A" xmlUrl="https://a.example/rss"/>
            </outline>
          </body>
        </opml>
        """#
        let imported = try OPMLImport().parseOPML(data: opml.data(using: .utf8)!)
        XCTAssertEqual(imported.count, 1)
        XCTAssertEqual(imported[0].title, "A")
    }

    // MARK: - Error cases

    func testMalformedXMLThrows() {
        let parser = RSSParser()
        let data = "<not valid".data(using: .utf8)!
        XCTAssertThrowsError(try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        ))
    }

    func testFeedWithoutChannelThrows() {
        let parser = RSSParser()
        let data = "<?xml version=\"1.0\"?><rss version=\"2.0\"></rss>".data(using: .utf8)!
        XCTAssertThrowsError(try parser.parse(
            data: data,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )) { error in
            guard case RSSParser.ParseError.missingChannel = error else {
                XCTFail("expected .missingChannel, got \(error)")
                return
            }
        }
    }

    // MARK: - TranscriptKind classification

    func testTranscriptKindClassifiesCommonMimeTypes() {
        XCTAssertEqual(TranscriptKind.from(mimeType: "text/vtt"), .vtt)
        XCTAssertEqual(TranscriptKind.from(mimeType: "text/vtt; charset=utf-8"), .vtt)
        XCTAssertEqual(TranscriptKind.from(mimeType: "application/json"), .json)
        XCTAssertEqual(TranscriptKind.from(mimeType: "application/x-subrip"), .srt)
        XCTAssertEqual(TranscriptKind.from(mimeType: "text/html"), .html)
        XCTAssertNil(TranscriptKind.from(mimeType: "application/octet-stream"))
        XCTAssertNil(TranscriptKind.from(mimeType: nil))
    }
}
