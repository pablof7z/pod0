import XCTest
@testable import Podcastr

final class RSSParserURLResolutionTests: XCTestCase {

    func testResolvesRelativeAndProtocolRelativeURLsAgainstFeedURL() throws {
        let xml = #"""
        <?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0"
             xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
             xmlns:podcast="https://podcastindex.org/namespace/1.0">
          <channel>
            <title>Relative Show</title>
            <itunes:image href="/art/cover.jpg"/>
            <item>
              <title>Episode</title>
              <pubDate>Mon, 04 May 2026 09:00:00 GMT</pubDate>
              <guid>relative-1</guid>
              <enclosure url="//media.example.net/audio/episode.mp3" type="audio/mpeg"/>
              <itunes:image href="images/episode.jpg"/>
              <podcast:transcript url="../transcripts/episode.vtt" type="text/vtt"/>
              <podcast:chapters url="/chapters/episode.json"/>
              <podcast:person role="host"
                              img="//cdn.example.net/host.jpg"
                              href="/people/alice">Alice</podcast:person>
            </item>
          </channel>
        </rss>
        """#

        let result = try RSSParser().parse(
            data: xml.data(using: .utf8)!,
            feedURL: URL(string: "https://feeds.example.com/podcasts/show/feed.xml")!
        )
        let episode = try XCTUnwrap(result.episodes.first)
        let person = try XCTUnwrap(episode.persons?.first)

        XCTAssertEqual(result.podcast.imageURL?.absoluteString, "https://feeds.example.com/art/cover.jpg")
        XCTAssertEqual(episode.enclosureURL.absoluteString, "https://media.example.net/audio/episode.mp3")
        XCTAssertEqual(episode.imageURL?.absoluteString, "https://feeds.example.com/podcasts/show/images/episode.jpg")
        XCTAssertEqual(episode.publisherTranscriptURL?.absoluteString, "https://feeds.example.com/podcasts/transcripts/episode.vtt")
        XCTAssertEqual(episode.chaptersURL?.absoluteString, "https://feeds.example.com/chapters/episode.json")
        XCTAssertEqual(person.imageURL?.absoluteString, "https://cdn.example.net/host.jpg")
        XCTAssertEqual(person.linkURL?.absoluteString, "https://feeds.example.com/people/alice")
    }

    func testMissingOrMalformedPubDateFallsBackToStableOldDate() throws {
        let xml = #"""
        <?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Date Fallback Show</title>
            <item>
              <title>Malformed</title>
              <pubDate>not a date</pubDate>
              <guid>bad-date</guid>
              <enclosure url="https://example.com/bad.mp3" type="audio/mpeg"/>
            </item>
            <item>
              <title>Missing</title>
              <guid>missing-date</guid>
              <enclosure url="https://example.com/missing.mp3" type="audio/mpeg"/>
            </item>
          </channel>
        </rss>
        """#

        let result = try RSSParser().parse(
            data: xml.data(using: .utf8)!,
            feedURL: URL(string: "https://example.com/feed.xml")!
        )
        let fallback = Date(timeIntervalSince1970: 0)

        XCTAssertEqual(result.episodes.map(\.pubDate), [fallback, fallback])
    }
}
