import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedLibraryExternalEpisodeTests: XCTestCase {
    func testExternalRSSEpisodeAndMetadataHydrationRemainCoreOwned() async throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        let feedURL = URL(string: "https://external.example/feed.xml")!
        let host = QueuedCoreFeedHost([.feedBytesFetched(
            bytes: Data(Self.metadataFeed.utf8),
            entityTag: "\"metadata-v1\"",
            lastModified: nil,
            responseUrl: feedURL.absoluteString,
            httpStatus: 200
        )])
        let store = AppStateStore(
            persistence: persistence,
            sharedLibraryMode: .automatic,
            sharedFeedHost: host,
            startPeriodicSubscriptionRefresh: false
        )
        let requestedPodcastID = UUID()
        let audioURL = URL(string: "https://external.example/selected.mp3")!

        let episode = try await store.upsertExternalEpisodeAndWait(
            podcastID: requestedPodcastID,
            feedURL: feedURL,
            podcastTitle: "external.example",
            audioURL: audioURL,
            title: "Selected Episode",
            imageURL: nil,
            duration: 91
        )
        XCTAssertEqual(episode.podcastID, requestedPodcastID)
        XCTAssertEqual(episode.enclosureURL, audioURL)
        XCTAssertEqual(store.podcast(id: requestedPodcastID)?.titleIsPlaceholder, true)
        XCTAssertNil(store.subscription(podcastID: requestedPodcastID))

        let sharedLibrary = try XCTUnwrap(store.sharedLibrary)
        _ = try await sharedLibrary.execute(.hydratePodcastMetadata(
            podcastId: PodcastId(uuid: requestedPodcastID)
        ))
        XCTAssertEqual(store.podcast(id: requestedPodcastID)?.title, "Hydrated External Show")
        XCTAssertEqual(store.podcast(id: requestedPodcastID)?.titleIsPlaceholder, false)
        XCTAssertEqual(store.podcast(id: requestedPodcastID)?.etag, "\"metadata-v1\"")
        XCTAssertEqual(store.episodes(forPodcast: requestedPodcastID).map(\.guid), [
            audioURL.absoluteString
        ])
        XCTAssertNil(store.subscription(podcastID: requestedPodcastID))

        let relaunched = AppStateStore(
            persistence: persistence,
            sharedLibraryMode: .automatic,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startPeriodicSubscriptionRefresh: false
        )
        XCTAssertEqual(relaunched.podcast(id: requestedPodcastID)?.title, "Hydrated External Show")
        XCTAssertEqual(relaunched.episodes(forPodcast: requestedPodcastID).count, 1)
        XCTAssertNil(relaunched.subscription(podcastID: requestedPodcastID))
    }

    private static let metadataFeed = #"""
    <?xml version="1.0" encoding="UTF-8"?>
    <rss version="2.0">
      <channel>
        <title>Hydrated External Show</title>
        <item>
          <title>Backlog Must Not Be Added</title>
          <guid>feed-backlog</guid>
          <pubDate>Sat, 18 Jul 2026 12:00:00 GMT</pubDate>
          <enclosure url="https://external.example/backlog.mp3" type="audio/mpeg" />
        </item>
      </channel>
    </rss>
    """#
}
