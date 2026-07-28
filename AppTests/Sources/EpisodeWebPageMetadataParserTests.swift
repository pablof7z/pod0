import XCTest
@testable import Podcastr

@MainActor
final class EpisodeWebPageMetadataParserTests: XCTestCase {
    func testParsesOvercastEpisodeMetadata() throws {
        let html = """
        <html>
          <head>
            <link rel="canonical" href="https://example.com/episodes/42">
            <meta property="og:title" content="A Better Episode &mdash; Test Show">
            <meta property="og:description" content="A useful conversation &amp; notes.">
            <meta property="og:image" content="https://example.com/art.jpg">
            <meta name="twitter:player:stream" content="https://cdn.example.com/42.mp3#t=0">
            <meta name="twitter:player:stream:content_type" content="audio/mpeg">
          </head>
          <body>
            <a href="https://podcasts.apple.com/podcast/id123456789">Apple Podcasts</a>
          </body>
        </html>
        """
        let metadata = EpisodeWebPageMetadataParser.parse(
            data: Data(html.utf8),
            baseURL: URL(string: "https://overcast.fm/+abc123")!
        )

        XCTAssertEqual(metadata.episodeTitle, "A Better Episode")
        XCTAssertEqual(metadata.podcastTitle, "Test Show")
        XCTAssertEqual(metadata.description, "A useful conversation & notes.")
        XCTAssertEqual(metadata.audioURL, URL(string: "https://cdn.example.com/42.mp3#t=0"))
        XCTAssertEqual(metadata.audioMIMEType, "audio/mpeg")
        XCTAssertEqual(metadata.imageURL, URL(string: "https://example.com/art.jpg"))
        XCTAssertEqual(metadata.canonicalURL, URL(string: "https://example.com/episodes/42"))
        XCTAssertEqual(metadata.applePodcastID, "123456789")
    }

    func testParsesApplePodcastEpisodeStructuredData() throws {
        let html = """
        <html><head>
          <script type="application/ld+json">
          {
            "@context": "https://schema.org",
            "@type": "PodcastEpisode",
            "name": "The Episode",
            "description": "Deep conversation",
            "datePublished": "2026-07-26T12:30:00Z",
            "duration": "PT1H2M3S",
            "thumbnailUrl": "https://example.com/episode.jpg",
            "partOfSeries": {"@type": "CreativeWorkSeries", "name": "The Show"}
          }
          </script>
        </head><body>
          <script>
          {"contentId":"1000123456789","feedUrl":"https:\\/\\/example.com\\/feed.xml",
           "guid":"episode-guid","streamUrl":"https:\\/\\/cdn.example.com\\/episode.m4a"}
          </script>
        </body></html>
        """
        let metadata = EpisodeWebPageMetadataParser.parse(
            data: Data(html.utf8),
            baseURL: URL(
                string: "https://podcasts.apple.com/us/podcast/the-episode/id987654321?i=1000123456789"
            )!
        )

        XCTAssertEqual(metadata.episodeTitle, "The Episode")
        XCTAssertEqual(metadata.podcastTitle, "The Show")
        XCTAssertEqual(metadata.description, "Deep conversation")
        XCTAssertEqual(metadata.duration, 3_723)
        XCTAssertEqual(
            metadata.publishedAt,
            ISO8601DateFormatter().date(from: "2026-07-26T12:30:00Z")
        )
        XCTAssertEqual(metadata.feedURL, URL(string: "https://example.com/feed.xml"))
        XCTAssertEqual(metadata.audioURL, URL(string: "https://cdn.example.com/episode.m4a"))
        XCTAssertEqual(metadata.guid, "episode-guid")
        XCTAssertEqual(metadata.applePodcastID, "987654321")
    }

    func testParsesGenericRSSAndAudioLinks() {
        let html = """
        <link type='application/rss+xml' rel='alternate' href='/podcast.xml'>
        <meta property='og:title' content='Shared Episode'>
        <audio controls src='/media/shared.mp3'></audio>
        """
        let metadata = EpisodeWebPageMetadataParser.parse(
            data: Data(html.utf8),
            baseURL: URL(string: "https://publisher.example/episodes/shared")!
        )

        XCTAssertEqual(
            metadata.feedURL,
            URL(string: "https://publisher.example/podcast.xml")
        )
        XCTAssertEqual(
            metadata.audioURL,
            URL(string: "https://publisher.example/media/shared.mp3")
        )
    }
}
