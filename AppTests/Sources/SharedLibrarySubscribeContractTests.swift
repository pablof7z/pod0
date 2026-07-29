import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

/// Swift-level mirror of the Rust
/// `subscribe_commits_durably_before_any_feed_fetch_observation` contract:
/// `addSubscription` returns once the subscription is durably committed,
/// before any feed bytes have been observed, and hydration arrives by
/// driving the host pump afterwards.
@MainActor
final class SharedLibrarySubscribeContractTests: XCTestCase {
    func testAddSubscriptionReturnsBeforeFetchAndHydratesAfterPumpDrive() async throws {
        let feedURL = "https://contract.example/feed.xml"
        // The gate withholds the only feed response, so nothing observed
        // before `release()` can come from a fetch.
        let host = GatedCoreFeedHost(.feedBytesFetched(
            bytes: Data(Self.feed.utf8),
            entityTag: "\"v1\"",
            lastModified: nil,
            responseUrl: feedURL,
            httpStatus: 200
        ))
        let made = AppStateTestSupport.makeIsolatedStore(sharedFeedHost: host)
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let store = made.store
        let client = try XCTUnwrap(store.sharedLibrary)

        let podcast = try await SubscriptionService(store: store)
            .addSubscription(feedURLString: feedURL)

        // Returned while the fetch is still gated: the subscription and its
        // durable fetch workflow are committed, and no episodes exist yet.
        let envelope = await client.coreSnapshot(ProjectionRequest(
            scope: .library,
            offset: 0,
            maxItems: 20
        ))
        guard case .library(let committed) = envelope.projection else {
            return XCTFail("Expected a library projection")
        }
        XCTAssertEqual(committed.subscriptions.map(\.podcastId.uuid), [podcast.id])
        XCTAssertTrue(committed.episodes.isEmpty)
        XCTAssertTrue(committed.feedFetches.contains {
            $0.podcastId.uuid == podcast.id && $0.stage == .requested
        })

        await host.release()
        await AppStateTestSupport.settleSharedFeedWork(store)

        XCTAssertEqual(store.podcast(id: podcast.id)?.title, "Gated Show")
        XCTAssertEqual(
            store.episodes(forPodcast: podcast.id).map(\.guid),
            ["gated-episode-1"]
        )
        XCTAssertEqual(store.state.subscriptions.count, 1)
        XCTAssertFalse(store.isFeedFetchInFlight(podcastID: podcast.id))
    }

    private static let feed = #"""
    <?xml version="1.0" encoding="UTF-8"?>
    <rss version="2.0">
      <channel>
        <title>Gated Show</title>
        <item>
          <title>First Episode</title>
          <guid>gated-episode-1</guid>
          <pubDate>Sat, 18 Jul 2026 12:00:00 GMT</pubDate>
          <enclosure url="https://cdn.example/gated-episode-1.mp3" type="audio/mpeg" />
        </item>
      </channel>
    </rss>
    """#
}
