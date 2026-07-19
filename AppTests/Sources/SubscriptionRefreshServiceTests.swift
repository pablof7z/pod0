import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SubscriptionRefreshServiceTests: XCTestCase {
    func testSubscriptionServiceRefreshUsesSharedRefreshSemantics() async throws {
        let feedURL = "https://feeds.example.com/show.xml"
        let host = QueuedCoreFeedHost([
            response(
                feedURL: feedURL,
                title: "Old Title",
                guid: "episode-0",
                entityTag: "\"v1\""
            ),
            response(
                feedURL: feedURL,
                title: "Fresh Title",
                guid: "episode-1",
                entityTag: "\"v2\""
            )
        ])
        let made = AppStateTestSupport.makeIsolatedStore(sharedFeedHost: host)
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let service = SubscriptionService(store: made.store)
        let podcast = try await service.addSubscription(feedURLString: feedURL)

        await service.refresh(podcast)

        let refreshed = try XCTUnwrap(made.store.podcast(id: podcast.id))
        XCTAssertEqual(refreshed.title, "Fresh Title")
        XCTAssertEqual(refreshed.etag, "\"v2\"")
        XCTAssertEqual(
            Set(made.store.episodes(forPodcast: podcast.id).map(\.guid)),
            Set(["episode-0", "episode-1"])
        )
    }

    func testRefreshServiceNotModifiedUpdatesValidatorsWithoutReplacingEpisodes() async throws {
        let feedURL = "https://feeds.example.com/show.xml"
        let host = QueuedCoreFeedHost([
            response(
                feedURL: feedURL,
                title: "Show",
                guid: "episode-1",
                entityTag: "\"v1\""
            ),
            .feedNotModified(
                entityTag: "\"v2\"",
                lastModified: "Sun, 19 Jul 2026 01:00:00 GMT",
                responseUrl: feedURL
            )
        ])
        let made = AppStateTestSupport.makeIsolatedStore(sharedFeedHost: host)
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = try await SubscriptionService(store: made.store)
            .addSubscription(feedURLString: feedURL)
        let originalRefresh = try XCTUnwrap(
            made.store.podcast(id: podcast.id)?.lastRefreshedAt
        )

        try await SubscriptionRefreshService().refresh(podcast.id, store: made.store)

        let updated = try XCTUnwrap(made.store.podcast(id: podcast.id))
        XCTAssertGreaterThanOrEqual(
            updated.lastRefreshedAt ?? .distantPast,
            originalRefresh
        )
        XCTAssertEqual(updated.etag, "\"v2\"")
        XCTAssertEqual(
            made.store.episodes(forPodcast: podcast.id).map(\.guid),
            ["episode-1"]
        )
    }

    private func response(
        feedURL: String,
        title: String,
        guid: String,
        entityTag: String
    ) -> HostObservation {
        .feedBytesFetched(
            bytes: Data(Self.feedXML(title: title, guid: guid).utf8),
            entityTag: entityTag,
            lastModified: nil,
            responseUrl: feedURL,
            httpStatus: 200
        )
    }

    private static func feedXML(title: String, guid: String) -> String {
        #"""
        <?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>\#(title)</title>
            <item>
              <title>Episode</title>
              <pubDate>Mon, 04 May 2026 09:00:00 GMT</pubDate>
              <guid>\#(guid)</guid>
              <enclosure url="https://cdn.example.com/\#(guid).mp3" type="audio/mpeg"/>
            </item>
          </channel>
        </rss>
        """#
    }
}
