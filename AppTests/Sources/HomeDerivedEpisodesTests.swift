import XCTest
@testable import Podcastr

/// Coverage for the `AppStateStore` derived views Home Today depends on:
/// `inProgressEpisodes` (Continue Listening rail) and
/// `recentEpisodes(limit:)` (New Episodes feed). Both apply a position
/// cache fold and have non-trivial filter + sort semantics, so they need
/// direct test coverage independent of the SwiftUI layer.
@MainActor
final class HomeDerivedEpisodesTests: XCTestCase {

    private var fileURL: URL!
    private var store: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        fileURL = made.fileURL
        store = made.store
    }

    override func tearDown() async throws {
        if let fileURL {
            AppStateTestSupport.disposeIsolatedStore(at: fileURL)
        }
        store = nil
        fileURL = nil
        try await super.tearDown()
    }

    // MARK: - inProgressEpisodes

    func testInProgressIncludesPartiallyListenedUnplayedEpisodes() {
        let sub = seedSubscription()
        let ep = makeEpisode(podcastID: sub.id, guid: "ip-1")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        store.setEpisodePlaybackPosition(ep.id, position: 600)

        let inProgress = store.inProgressEpisodes

        XCTAssertEqual(inProgress.count, 1)
        XCTAssertEqual(inProgress.first?.id, ep.id)
    }

    func testInProgressExcludesPlayedEpisodes() {
        let sub = seedSubscription()
        let ep = makeEpisode(podcastID: sub.id, guid: "ip-played")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        store.setEpisodePlaybackPosition(ep.id, position: 600)
        store.markEpisodePlayed(ep.id)
        // mark-played zeroes position too — this also exercises the
        // intersection of "played" and "position == 0" excluders.

        XCTAssertTrue(store.inProgressEpisodes.isEmpty)
    }

    func testInProgressExcludesUnplayedZeroPositionEpisodes() {
        let sub = seedSubscription()
        let ep = makeEpisode(podcastID: sub.id, guid: "ip-zero")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        // No setEpisodePlaybackPosition — position stays at 0.

        XCTAssertTrue(store.inProgressEpisodes.isEmpty)
    }

    func testInProgressSortedNewestPubDateFirst() {
        let sub = seedSubscription()
        let now = Date()
        var older = makeEpisode(podcastID: sub.id, guid: "ip-older")
        older.pubDate = now.addingTimeInterval(-7 * 86_400)
        var newer = makeEpisode(podcastID: sub.id, guid: "ip-newer")
        newer.pubDate = now
        store.upsertEpisodes([older, newer], forPodcast: sub.id)
        store.setEpisodePlaybackPosition(older.id, position: 1)
        store.setEpisodePlaybackPosition(newer.id, position: 1)

        let inProgress = store.inProgressEpisodes
        XCTAssertEqual(inProgress.map(\.id), [newer.id, older.id])
    }

    // MARK: - recentEpisodes(limit:)

    func testRecentEpisodesExcludesPlayed() {
        let sub = seedSubscription()
        let played = makeEpisode(podcastID: sub.id, guid: "rec-played")
        let unplayed = makeEpisode(podcastID: sub.id, guid: "rec-unplayed")
        store.upsertEpisodes([played, unplayed], forPodcast: sub.id)
        store.markEpisodePlayed(played.id)

        let recent = store.recentEpisodes(limit: 30)

        XCTAssertEqual(recent.count, 1)
        XCTAssertEqual(recent.first?.id, unplayed.id)
    }

    func testRecentEpisodesSortedNewestFirst() {
        let sub = seedSubscription()
        let now = Date()
        var first = makeEpisode(podcastID: sub.id, guid: "rec-1")
        first.pubDate = now.addingTimeInterval(-30 * 86_400)
        var second = makeEpisode(podcastID: sub.id, guid: "rec-2")
        second.pubDate = now.addingTimeInterval(-15 * 86_400)
        var third = makeEpisode(podcastID: sub.id, guid: "rec-3")
        third.pubDate = now
        store.upsertEpisodes([first, second, third], forPodcast: sub.id)

        let recent = store.recentEpisodes(limit: 30)

        XCTAssertEqual(recent.map(\.id), [third.id, second.id, first.id])
    }

    func testRecentEpisodesRespectsLimit() {
        let sub = seedSubscription()
        let now = Date()
        let episodes = (0..<10).map { i -> Episode in
            var ep = makeEpisode(podcastID: sub.id, guid: "rec-\(i)")
            ep.pubDate = now.addingTimeInterval(-Double(i) * 86_400)
            return ep
        }
        store.upsertEpisodes(episodes, forPodcast: sub.id)

        let limited = store.recentEpisodes(limit: 3)

        XCTAssertEqual(limited.count, 3)
        // Newest 3 — guids 0, 1, 2.
        XCTAssertEqual(limited.map(\.guid), ["rec-0", "rec-1", "rec-2"])
    }

    func testRecentEpisodesIncludesInProgressEpisodes() {
        // "Recent" filters on `!played` only — half-listened episodes still
        // surface, which matches the Today/New-episodes UX (the user can
        // see something they started but haven't finished).
        let sub = seedSubscription()
        let ep = makeEpisode(podcastID: sub.id, guid: "rec-half")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        store.setEpisodePlaybackPosition(ep.id, position: 1234)

        let recent = store.recentEpisodes(limit: 30)
        XCTAssertEqual(recent.first?.id, ep.id)
    }

    // MARK: - Fixtures

    private func seedSubscription() -> Podcast {
        let podcast = Podcast(
            feedURL: URL(string: "https://example.com/\(UUID().uuidString).xml")!,
            title: "Home Derived Test Show"
        )
        let stored = store.upsertPodcast(podcast)
        store.addSubscription(podcastID: stored.id)
        return stored
    }

    private func makeEpisode(podcastID: UUID, guid: String) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: guid,
            title: "Episode \(guid)",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }
}
