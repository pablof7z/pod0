import XCTest
@testable import Podcastr

/// Pure-value tests for `ITunesSearchClient.Result`. Verifies the artwork-URL
/// preference logic and the feed-URL coercion — both of which the rest of
/// the Discover surface relies on without re-checking.
@MainActor
final class ITunesSearchClientTests: XCTestCase {

    // MARK: - artworkURL preference

    func testArtworkURLPrefers600OverThe100() throws {
        let result = ITunesSearchClient.Result(
            collectionId: 1,
            collectionName: "Show",
            artistName: nil,
            feedUrl: "https://example.com/feed.xml",
            artworkUrl600: "https://example.com/600.jpg",
            artworkUrl100: "https://example.com/100.jpg",
            primaryGenreName: nil,
            trackCount: nil
        )

        XCTAssertEqual(result.artworkURL, URL(string: "https://example.com/600.jpg"))
    }

    func testArtworkURLFallsBackToThe100() throws {
        let result = ITunesSearchClient.Result(
            collectionId: 1,
            collectionName: "Show",
            artistName: nil,
            feedUrl: "https://example.com/feed.xml",
            artworkUrl600: nil,
            artworkUrl100: "https://example.com/100.jpg",
            primaryGenreName: nil,
            trackCount: nil
        )

        XCTAssertEqual(result.artworkURL, URL(string: "https://example.com/100.jpg"))
    }

    func testArtworkURLNilWhenBothMissing() throws {
        let result = ITunesSearchClient.Result(
            collectionId: 1,
            collectionName: "Show",
            artistName: nil,
            feedUrl: "https://example.com/feed.xml",
            artworkUrl600: nil,
            artworkUrl100: nil,
            primaryGenreName: nil,
            trackCount: nil
        )

        XCTAssertNil(result.artworkURL)
    }

    func testArtworkURLNilWhenStringIsMalformed() throws {
        // `URL(string:)` accepts surprisingly malformed inputs, so use a
        // value Foundation actually rejects (empty string).
        let result = ITunesSearchClient.Result(
            collectionId: 1,
            collectionName: "Show",
            artistName: nil,
            feedUrl: "https://example.com/feed.xml",
            artworkUrl600: "",
            artworkUrl100: "",
            primaryGenreName: nil,
            trackCount: nil
        )

        XCTAssertNil(result.artworkURL)
    }

    // MARK: - feedURL coercion

    func testFeedURLCoercesValidString() throws {
        let result = makeResult(feedUrl: "https://example.com/feed.xml")
        XCTAssertEqual(result.feedURL, URL(string: "https://example.com/feed.xml"))
    }

    func testFeedURLNilWhenMissing() throws {
        let result = makeResult(feedUrl: nil)
        XCTAssertNil(result.feedURL)
    }

    // MARK: - id mirrors collectionId

    func testIDMirrorsCollectionID() throws {
        let result = makeResult(collectionId: 42)
        XCTAssertEqual(result.id, 42)
    }

    // MARK: - JSON round-trip (smoke test for the decoder)

    func testDecodesAppleSearchResponseShape() throws {
        let json = """
        {
          "collectionId": 12345,
          "collectionName": "Test Show",
          "artistName": "Test Host",
          "feedUrl": "https://example.com/feed.xml",
          "artworkUrl600": "https://example.com/600.jpg",
          "artworkUrl100": "https://example.com/100.jpg",
          "primaryGenreName": "Technology",
          "trackCount": 200
        }
        """.data(using: .utf8)!

        let decoded = try JSONDecoder().decode(ITunesSearchClient.Result.self, from: json)

        XCTAssertEqual(decoded.collectionId, 12345)
        XCTAssertEqual(decoded.collectionName, "Test Show")
        XCTAssertEqual(decoded.artistName, "Test Host")
        XCTAssertEqual(decoded.primaryGenreName, "Technology")
        XCTAssertEqual(decoded.trackCount, 200)
        XCTAssertEqual(decoded.feedURL, URL(string: "https://example.com/feed.xml"))
    }

    // MARK: - Fixtures

    private func makeResult(
        collectionId: Int = 1,
        feedUrl: String? = "https://example.com/feed.xml"
    ) -> ITunesSearchClient.Result {
        ITunesSearchClient.Result(
            collectionId: collectionId,
            collectionName: "Show",
            artistName: nil,
            feedUrl: feedUrl,
            artworkUrl600: nil,
            artworkUrl100: nil,
            primaryGenreName: nil,
            trackCount: nil
        )
    }
}
