import XCTest
@testable import Podcastr

@MainActor
final class SharedEpisodeResolverTests: XCTestCase {
    func testOvercastShareResolvesThroughAppleDirectoryAndRSS() async throws {
        let pageURL = URL(string: "https://overcast.fm/+episode")!
        let feedURL = URL(string: "https://feeds.example.com/show.xml")!
        let audioURL = URL(string: "https://cdn.example.com/selected.mp3")!
        var lookup = URLComponents(string: "https://itunes.apple.com/lookup")!
        lookup.queryItems = [
            URLQueryItem(name: "id", value: "617416468"),
            URLQueryItem(name: "entity", value: "podcast")
        ]
        let lookupURL = try XCTUnwrap(lookup.url)
        let stub = SharedEpisodeLoaderStub(documents: [
            pageURL: .html(
                """
                <meta property="og:title" content="Selected Episode &mdash; Test Show">
                <meta property="og:description" content="Page description">
                <meta property="og:image" content="https://example.com/page-art.jpg">
                <meta name="twitter:player:stream" content="\(audioURL.absoluteString)#t=0">
                <a href="https://podcasts.apple.com/podcast/id617416468">Apple</a>
                """,
                url: pageURL
            ),
            lookupURL: .json(
                #"{"resultCount":1,"results":[{"feedUrl":"https://feeds.example.com/show.xml"}]}"#,
                url: lookupURL
            ),
            feedURL: .xml(
                """
                <rss version="2.0"><channel>
                  <title>Test Show from RSS</title>
                  <image><url>https://example.com/feed-art.jpg</url></image>
                  <item>
                    <title>Selected Episode</title>
                    <description>RSS description</description>
                    <pubDate>Sun, 26 Jul 2026 12:00:00 GMT</pubDate>
                    <itunes:duration>42:10</itunes:duration>
                    <enclosure url="\(audioURL.absoluteString)" type="audio/mpeg"/>
                  </item>
                </channel></rss>
                """,
                url: feedURL
            )
        ])
        let resolver = SharedEpisodeResolver { url, _, _ in
            try await stub.document(for: url)
        }

        let result = try await resolver.resolve(pageURL)

        XCTAssertEqual(result.podcastTitle, "Test Show from RSS")
        XCTAssertEqual(result.title, "Selected Episode")
        XCTAssertEqual(result.description, "RSS description")
        XCTAssertEqual(result.feedURL, feedURL)
        XCTAssertEqual(result.audioURL, audioURL)
        XCTAssertEqual(result.enclosureMIMEType, "audio/mpeg")
        XCTAssertEqual(result.duration, 2_530)
        XCTAssertEqual(result.imageURL, URL(string: "https://example.com/page-art.jpg"))
    }

    func testDirectAudioURLImportsWithoutNetworkRequest() async throws {
        let stub = SharedEpisodeLoaderStub(documents: [:])
        let resolver = SharedEpisodeResolver { url, _, _ in
            try await stub.document(for: url)
        }
        let url = URL(string: "https://media.example.com/a-good-episode.mp3#t=42")!

        let result = try await resolver.resolve(url)

        XCTAssertEqual(result.title, "a good episode")
        XCTAssertEqual(result.podcastTitle, "media.example.com")
        XCTAssertEqual(
            result.audioURL,
            URL(string: "https://media.example.com/a-good-episode.mp3")
        )
        XCTAssertEqual(result.enclosureMIMEType, "audio/mpeg")
        let requestCount = await stub.requestCount
        XCTAssertEqual(requestCount, 0)
    }

    func testPageWithoutPlayableAudioFailsClearly() async throws {
        let pageURL = URL(string: "https://example.com/article")!
        let stub = SharedEpisodeLoaderStub(documents: [
            pageURL: .html("<meta property='og:title' content='Article'>", url: pageURL)
        ])
        let resolver = SharedEpisodeResolver { url, _, _ in
            try await stub.document(for: url)
        }

        do {
            _ = try await resolver.resolve(pageURL)
            XCTFail("Expected noPlayableEpisode")
        } catch let error as SharedEpisodeResolver.ResolveError {
            XCTAssertEqual(error, .noPlayableEpisode)
        }
    }
}

private actor SharedEpisodeLoaderStub {
    enum StubError: Error {
        case missing(URL)
    }

    private let documents: [URL: SharedEpisodeHTTPDocument]
    private(set) var requestCount = 0

    init(documents: [URL: SharedEpisodeHTTPDocument]) {
        self.documents = documents
    }

    func document(for url: URL) throws -> SharedEpisodeHTTPDocument {
        requestCount += 1
        guard let document = documents[url] else { throw StubError.missing(url) }
        return document
    }
}

private extension SharedEpisodeHTTPDocument {
    static func html(_ body: String, url: URL) -> Self {
        .init(data: Data(body.utf8), finalURL: url, mimeType: "text/html", statusCode: 200)
    }

    static func xml(_ body: String, url: URL) -> Self {
        .init(data: Data(body.utf8), finalURL: url, mimeType: "application/rss+xml", statusCode: 200)
    }

    static func json(_ body: String, url: URL) -> Self {
        .init(data: Data(body.utf8), finalURL: url, mimeType: "application/json", statusCode: 200)
    }
}
