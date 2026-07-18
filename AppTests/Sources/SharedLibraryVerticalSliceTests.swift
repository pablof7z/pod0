import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedLibraryVerticalSliceTests: XCTestCase {
    func testLegacyLibraryCutsOverOnceAndIgnoresStaleSwiftStateAfterRestart() async throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }

        let podcast = Podcast(
            id: UUID(uuidString: "10000000-0000-0000-0000-000000000001")!,
            feedURL: URL(string: "https://legacy.example/feed.xml")!,
            title: "Migrated Show",
            author: "Pod0",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let episode = Episode(
            id: UUID(uuidString: "20000000-0000-0000-0000-000000000001")!,
            podcastID: podcast.id,
            guid: "legacy-guid",
            title: "Migrated Episode",
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            duration: 600,
            enclosureURL: URL(string: "https://legacy.example/episode.mp3")!,
            playbackPosition: 33,
            isStarred: true
        )
        var legacy = AppState()
        legacy.podcasts = [podcast]
        legacy.subscriptions = [PodcastSubscription(
            podcastID: podcast.id,
            autoDownload: AutoDownloadPolicy(mode: .latestN(2), wifiOnly: true),
            notificationsEnabled: true
        )]
        legacy.episodes = [episode]
        XCTAssertTrue(persistence.write(legacy, revision: 1))

        let first = AppStateStore(
            persistence: persistence,
            sharedLibraryMode: .automatic,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startPeriodicSubscriptionRefresh: false
        )
        XCTAssertTrue(first.isSharedLibraryAuthoritative)
        XCTAssertNil(first.sharedLibraryUnavailableReason)
        XCTAssertEqual(first.podcast(id: podcast.id)?.title, "Migrated Show")
        XCTAssertEqual(first.episode(id: episode.id)?.playbackPosition, 33)
        XCTAssertEqual(first.episode(id: episode.id)?.isStarred, true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: persistence.sharedCoreStoreURL.path))

        try await first.setSubscriptionNotificationsAndWait(podcast.id, enabled: false)
        XCTAssertEqual(first.subscription(podcastID: podcast.id)?.notificationsEnabled, false)

        var stale = AppState()
        var stalePodcast = podcast
        stalePodcast.title = "STALE SWIFT TITLE"
        stale.podcasts = [stalePodcast]
        stale.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
        stale.episodes = []
        XCTAssertTrue(persistence.write(stale, revision: 10_000))

        let relaunched = AppStateStore(
            persistence: persistence,
            sharedLibraryMode: .automatic,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startPeriodicSubscriptionRefresh: false
        )
        XCTAssertTrue(relaunched.isSharedLibraryAuthoritative)
        XCTAssertEqual(relaunched.podcast(id: podcast.id)?.title, "Migrated Show")
        XCTAssertEqual(relaunched.episode(id: episode.id)?.playbackPosition, 33)
        XCTAssertEqual(relaunched.subscription(podcastID: podcast.id)?.notificationsEnabled, false)

        var rejectedPodcastWrite = try XCTUnwrap(relaunched.podcast(id: podcast.id))
        rejectedPodcastWrite.title = "DIRECT SWIFT WRITE"
        relaunched.updatePodcast(rejectedPodcastWrite)
        XCTAssertEqual(relaunched.podcast(id: podcast.id)?.title, "Migrated Show")
        XCTAssertTrue(relaunched.upsertEpisodes([
            Episode(
                podcastID: podcast.id,
                guid: "swift-only",
                title: "Must Not Persist",
                pubDate: Date(),
                enclosureURL: URL(string: "https://legacy.example/swift-only.mp3")!
            )
        ], forPodcast: podcast.id).isEmpty)

        try await relaunched.deletePodcastAndWait(podcastID: podcast.id)
        XCTAssertNil(relaunched.podcast(id: podcast.id))

        let afterDeletionRestart = AppStateStore(
            persistence: persistence,
            sharedLibraryMode: .automatic,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startPeriodicSubscriptionRefresh: false
        )
        XCTAssertNil(afterDeletionRestart.podcast(id: podcast.id))
        XCTAssertTrue(afterDeletionRestart.state.subscriptions.isEmpty)
        XCTAssertTrue(afterDeletionRestart.state.episodes.isEmpty)
    }

    func testSubscribeRefreshAndFeedDiscoveryConvergeThroughSharedCore() async throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        let feedURL = "https://feeds.example/shared.xml"
        let host = QueuedCoreFeedHost([
            .feedBytesFetched(
                bytes: Data(Self.feed(version: 1).utf8),
                entityTag: "\"v1\"",
                lastModified: "Sat, 18 Jul 2026 12:00:00 GMT",
                responseUrl: feedURL,
                httpStatus: 200
            ),
            .feedBytesFetched(
                bytes: Data(Self.feed(version: 2).utf8),
                entityTag: "\"v2\"",
                lastModified: "Sat, 18 Jul 2026 13:00:00 GMT",
                responseUrl: feedURL,
                httpStatus: 200
            )
        ])
        let store = AppStateStore(
            persistence: persistence,
            sharedLibraryMode: .automatic,
            sharedFeedHost: host,
            startPeriodicSubscriptionRefresh: false
        )
        XCTAssertTrue(store.isSharedLibraryAuthoritative)

        let service = SubscriptionService(store: store)
        let podcast = try await service.addSubscription(feedURLString: feedURL)
        XCTAssertEqual(podcast.title, "Shared Show")
        XCTAssertEqual(store.state.subscriptions.count, 1)
        XCTAssertEqual(store.episodes(forPodcast: podcast.id).map(\.guid), ["episode-1"])

        let firstEpisode = try XCTUnwrap(store.episodes(forPodcast: podcast.id).first)
        XCTAssertEqual(firstEpisode.publisherTranscriptType, .json)
        XCTAssertEqual(
            firstEpisode.publisherTranscriptURL?.absoluteString,
            "https://feeds.example/transcript.json"
        )
        XCTAssertEqual(firstEpisode.chaptersURL?.absoluteString, "https://feeds.example/chapters.json")
        XCTAssertEqual(firstEpisode.persons?.first?.name, "Ada Host")
        XCTAssertEqual(firstEpisode.soundBites?.first?.title, "Key idea")

        try await store.setSubscriptionAutoDownloadAndWait(
            podcast.id,
            policy: AutoDownloadPolicy(mode: .allNew, wifiOnly: false)
        )
        try await store.setSubscriptionNotificationsAndWait(podcast.id, enabled: true)
        try await SubscriptionRefreshService().refresh(podcast.id, store: store)

        XCTAssertEqual(
            Set(store.episodes(forPodcast: podcast.id).map(\.guid)),
            Set(["episode-1", "episode-2"])
        )
        XCTAssertEqual(store.podcast(id: podcast.id)?.etag, "\"v2\"")
        XCTAssertEqual(store.subscription(podcastID: podcast.id)?.autoDownload.mode, .allNew)

        let requests = await host.recordedRequests()
        XCTAssertEqual(requests.count, 2)
        XCTAssertNil(requests[0].entityTag)
        XCTAssertEqual(requests[1].entityTag, "\"v1\"")
        XCTAssertLessThanOrEqual(requests[0].maximumResponseBytes, 8 * 1_024 * 1_024)

        let discoveryJobs = try JobStore(fileURL: persistence.episodeStore.fileURL)
            .allJobs()
            .filter { $0.kind == .feedDiscovery }
        XCTAssertEqual(discoveryJobs.count, 1)
        let payload = try XCTUnwrap(discoveryJobs.first?.payload)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let discovery = try decoder.decode(FeedDiscoveryPayload.self, from: payload)
        XCTAssertEqual(discovery.episodes.map(\.title), ["Second Episode"])
        XCTAssertTrue(discovery.notificationsEnabled)
        XCTAssertEqual(discovery.autoDownloadPolicy?.mode, .allNew)
    }

    private static func feed(version: Int) -> String {
        let second = version > 1 ? #"""
            <item>
              <title>Second Episode</title>
              <guid>episode-2</guid>
              <pubDate>Sat, 18 Jul 2026 12:30:00 GMT</pubDate>
              <enclosure url="https://cdn.example/episode-2.mp3" type="audio/mpeg" />
            </item>
        """# : ""
        return #"""
        <?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0"
             xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
             xmlns:podcast="https://podcastindex.org/namespace/1.0">
          <channel>
            <title>Shared Show</title>
            <itunes:author>Pod0</itunes:author>
            <item>
              <title>First Episode</title>
              <guid>episode-1</guid>
              <pubDate>Sat, 18 Jul 2026 12:00:00 GMT</pubDate>
              <enclosure url="https://cdn.example/episode-1.mp3" type="audio/mpeg" />
              <podcast:transcript url="transcript.json" type="application/json" />
              <podcast:chapters url="chapters.json" type="application/json+chapters" />
              <podcast:person role="host">Ada Host</podcast:person>
              <podcast:soundbite startTime="12" duration="8">Key idea</podcast:soundbite>
            </item>
            \#(second)
          </channel>
        </rss>
        """#
    }
}

actor QueuedCoreFeedHost: CoreFeedHosting {
    struct Request: Sendable {
        let feedURL: String
        let entityTag: String?
        let lastModified: String?
        let maximumResponseBytes: UInt64
    }

    private var responses: [HostObservation]
    private var requests: [Request] = []

    init(_ responses: [HostObservation]) {
        self.responses = responses
    }

    func fetch(
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation {
        requests.append(Request(
            feedURL: feedURL,
            entityTag: entityTag,
            lastModified: lastModified,
            maximumResponseBytes: maximumResponseBytes
        ))
        guard !responses.isEmpty else {
            return .failed(code: .platformFailure, safeDetail: "No queued test response")
        }
        return responses.removeFirst()
    }

    func recordedRequests() -> [Request] { requests }
}
