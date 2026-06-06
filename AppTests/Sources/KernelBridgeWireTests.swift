import XCTest
@testable import Podcastr

final class KernelBridgeWireTests: XCTestCase {
    func testFeedActionPayloadsEncodeRustWireShape() throws {
        XCTAssertEqual(
            try PodcastKernelAction.Subscribe(feedUrl: "https://example.com/feed.xml")
                .bodyDictionary()["op"] as? String,
            "subscribe"
        )
        XCTAssertEqual(
            try PodcastKernelAction.EnsurePodcast(feedUrl: "https://example.com/feed.xml")
                .bodyDictionary()["op"] as? String,
            "ensure_podcast"
        )
        XCTAssertEqual(
            try PodcastKernelAction.RefreshAll().bodyDictionary()["op"] as? String,
            "refresh_all"
        )

        let refresh = try PodcastKernelAction.Refresh(podcastId: "pod-1").bodyDictionary()
        XCTAssertEqual(refresh["op"] as? String, "refresh")
        XCTAssertEqual(refresh["podcast_id"] as? String, "pod-1")

        let unsubscribe = try PodcastKernelAction.Unsubscribe(podcastId: "pod-1").bodyDictionary()
        XCTAssertEqual(unsubscribe["op"] as? String, "unsubscribe")
        XCTAssertEqual(unsubscribe["podcast_id"] as? String, "pod-1")
    }

    func testCreatePodcastOmitsNilOptionalFields() throws {
        let body = try PodcastKernelAction.CreatePodcast(
            podcastId: "pod-1",
            title: "Agent Show",
            description: "",
            author: "",
            feedUrl: nil,
            artworkUrl: nil,
            language: nil,
            categories: [],
            visibility: "private",
            titleIsPlaceholder: false
        ).bodyDictionary()

        XCTAssertEqual(body["op"] as? String, "create_podcast")
        XCTAssertEqual(body["podcast_id"] as? String, "pod-1")
        XCTAssertEqual(body["description"] as? String, "")
        XCTAssertEqual(body["author"] as? String, "")
        XCTAssertEqual(body["categories"] as? [String], [])
        XCTAssertNil(body["feed_url"])
        XCTAssertNil(body["artwork_url"])
        XCTAssertNil(body["language"])
        XCTAssertEqual(body["visibility"] as? String, "private")
        XCTAssertEqual(body["title_is_placeholder"] as? Bool, false)
    }

    func testAddEpisodeEncodesTypedChaptersAndOmitsNilOptionalFields() throws {
        let body = try PodcastKernelAction.AddEpisode(
            podcastId: "pod-1",
            episodeId: "ep-1",
            title: "Episode",
            enclosureUrl: "https://example.com/audio.mp3",
            description: "",
            durationSecs: nil,
            imageUrl: nil,
            chapters: [
                KernelEpisodeChapterPayload(
                    startSecs: 12.5,
                    title: "Clip",
                    imageUrl: "https://example.com/art.png",
                    sourceEpisodeId: "source-ep"
                )
            ],
            transcript: nil
        ).bodyDictionary()

        XCTAssertEqual(body["op"] as? String, "add_episode")
        XCTAssertEqual(body["enclosure_url"] as? String, "https://example.com/audio.mp3")
        XCTAssertNil(body["duration_secs"])
        XCTAssertNil(body["image_url"])
        XCTAssertNil(body["transcript"])
        let chapters = body["chapters"] as? [[String: Any]]
        XCTAssertEqual(chapters?.count, 1)
        XCTAssertEqual(chapters?.first?["start_secs"] as? Double, 12.5)
        XCTAssertEqual(chapters?.first?["source_episode_id"] as? String, "source-ep")
    }

    func testPodcastSummaryDecodesSubscriptionAndRefreshFields() throws {
        let data = Data("""
        {
          "id": "pod-1",
          "title": "Known Show",
          "is_subscribed": false,
          "last_refreshed_at": 1767225600000,
          "title_is_placeholder": true,
          "episodes": []
        }
        """.utf8)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase

        let summary = try decoder.decode(PodcastSummary.self, from: data)

        XCTAssertFalse(summary.isSubscribed)
        XCTAssertEqual(summary.lastRefreshedAt, 1_767_225_600_000)
        XCTAssertTrue(summary.titleIsPlaceholder)
    }
}
