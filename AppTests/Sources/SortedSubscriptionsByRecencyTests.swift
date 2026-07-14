import XCTest
@testable import Podcastr

/// Coverage for `AppStateStore.sortedFollowedPodcastsByRecency` — the new sort
/// driving the merged Home subscription list. Drops alphabetical ordering
/// in favour of "most-recently-active feed first" so a brand-new episode
/// surfaces at the top of the list, not buried behind 30 alphabetically-
/// earlier shows.
@MainActor
final class SortedSubscriptionsByRecencyTests: XCTestCase {

    private var fileURL: URL!
    private var store: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        fileURL = made.fileURL
        store = made.store
    }

    override func tearDown() async throws {
        if let fileURL { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        store = nil
        fileURL = nil
        try await super.tearDown()
    }

    func testSubscriptionsOrderedByMostRecentEpisodePubDate() {
        let alpha = makeSubscription(title: "Alpha")
        let bravo = makeSubscription(title: "Bravo")
        let charlie = makeSubscription(title: "Charlie")
        store.upsertPodcast(alpha); store.addSubscription(podcastID: alpha.id)
        store.upsertPodcast(bravo); store.addSubscription(podcastID: bravo.id)
        store.upsertPodcast(charlie); store.addSubscription(podcastID: charlie.id)

        let now = Date()
        store.upsertEpisodes(
            [makeEpisode(subID: alpha.id, guid: "a-1", pubDate: now.addingTimeInterval(-3 * 86_400))],
            forPodcast: alpha.id
        )
        store.upsertEpisodes(
            [makeEpisode(subID: bravo.id, guid: "b-1", pubDate: now)],
            forPodcast: bravo.id
        )
        store.upsertEpisodes(
            [makeEpisode(subID: charlie.id, guid: "c-1", pubDate: now.addingTimeInterval(-86_400))],
            forPodcast: charlie.id
        )

        let order = store.sortedFollowedPodcastsByRecency.map(\.title)
        XCTAssertEqual(order, ["Bravo", "Charlie", "Alpha"])
    }

    func testSubscriptionsWithoutEpisodesSinkToBottomAlphabetically() {
        let withEp = makeSubscription(title: "Zebra Show")
        let blank1 = makeSubscription(title: "Bravo")
        let blank2 = makeSubscription(title: "Alpha")
        store.upsertPodcast(withEp); store.addSubscription(podcastID: withEp.id)
        store.upsertPodcast(blank1); store.addSubscription(podcastID: blank1.id)
        store.upsertPodcast(blank2); store.addSubscription(podcastID: blank2.id)

        store.upsertEpisodes(
            [makeEpisode(subID: withEp.id, guid: "z-1", pubDate: Date())],
            forPodcast: withEp.id
        )

        let order = store.sortedFollowedPodcastsByRecency.map(\.title)
        XCTAssertEqual(order, ["Zebra Show", "Alpha", "Bravo"])
    }

    func testTieOnPubDateBreaksAlphabetically() {
        let alpha = makeSubscription(title: "Alpha")
        let bravo = makeSubscription(title: "Bravo")
        store.upsertPodcast(alpha); store.addSubscription(podcastID: alpha.id)
        store.upsertPodcast(bravo); store.addSubscription(podcastID: bravo.id)

        let pinned = Date()
        store.upsertEpisodes(
            [makeEpisode(subID: alpha.id, guid: "a-1", pubDate: pinned)],
            forPodcast: alpha.id
        )
        store.upsertEpisodes(
            [makeEpisode(subID: bravo.id, guid: "b-1", pubDate: pinned)],
            forPodcast: bravo.id
        )

        let order = store.sortedFollowedPodcastsByRecency.map(\.title)
        XCTAssertEqual(order, ["Alpha", "Bravo"])
    }

    func testMostRecentEpisodeMatchesProjection() {
        let sub = makeSubscription(title: "X")
        store.upsertPodcast(sub); store.addSubscription(podcastID: sub.id)
        let older = makeEpisode(subID: sub.id, guid: "older", pubDate: Date().addingTimeInterval(-86_400))
        let newer = makeEpisode(subID: sub.id, guid: "newer", pubDate: Date())
        store.upsertEpisodes([older, newer], forPodcast: sub.id)

        let recent = store.mostRecentEpisode(forPodcast: sub.id)
        XCTAssertEqual(recent?.guid, "newer")
    }

    // MARK: - Fixtures

    private func makeSubscription(title: String) -> Podcast {
        Podcast(
            feedURL: URL(string: "https://example.com/\(UUID().uuidString).xml")!,
            title: title
        )
    }

    private func makeEpisode(subID: UUID, guid: String, pubDate: Date) -> Episode {
        Episode(
            podcastID: subID,
            guid: guid,
            title: "Episode \(guid)",
            pubDate: pubDate,
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }
}
