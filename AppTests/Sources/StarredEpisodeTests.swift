import XCTest
@testable import Podcastr

/// Coverage for `AppStateStore`'s starred-episode API:
/// `toggleEpisodeStarred(_:)` and `setEpisodeStarred(_:_:)`.
///
/// Tests verify flag mutation, isolation between episodes, idempotent
/// guards, and the documented feed-upsert invariant: `isStarred` must
/// survive a feed refresh even when the publisher emits a fresh episode
/// object with the flag reset to `false`.
@MainActor
final class StarredEpisodeTests: XCTestCase {

    private var fileURL: URL!
    private var store: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        fileURL = made.fileURL
        store = made.store
    }

    override func tearDown() async throws {
        AppStateTestSupport.disposeIsolatedStore(at: fileURL)
        store = nil
        fileURL = nil
        try await super.tearDown()
    }

    // MARK: - toggleEpisodeStarred

    func testToggleStarredSetsTrueWhenUnstarred() {
        let ep = insertEpisode(guid: "ep1")
        XCTAssertFalse(store.episode(id: ep.id)?.isStarred ?? true)

        store.toggleEpisodeStarred(ep.id)

        XCTAssertTrue(store.episode(id: ep.id)?.isStarred ?? false)
    }

    func testToggleStarredSetsFalseWhenAlreadyStarred() {
        let ep = insertEpisode(guid: "ep2")
        store.setEpisodeStarred(ep.id, true)
        XCTAssertTrue(store.episode(id: ep.id)?.isStarred ?? false, "pre-condition")

        store.toggleEpisodeStarred(ep.id)

        XCTAssertFalse(store.episode(id: ep.id)?.isStarred ?? true)
    }

    func testToggleStarredRoundTrips() {
        let ep = insertEpisode(guid: "ep-rt")
        store.toggleEpisodeStarred(ep.id)
        store.toggleEpisodeStarred(ep.id)
        XCTAssertFalse(store.episode(id: ep.id)?.isStarred ?? true, "two toggles must restore original state")
    }

    func testToggleStarredIsNopForUnknownID() {
        let phantom = UUID()
        let ep = insertEpisode(guid: "ep-noop")
        store.toggleEpisodeStarred(phantom) // must not crash
        XCTAssertFalse(store.episode(id: ep.id)?.isStarred ?? true, "existing episode must be unaffected")
    }

    func testToggleStarredDoesNotAffectOtherEpisodes() {
        let sub = addSubscription(title: "Isolation")
        let ep1 = insertEpisode(guid: "iso-1", in: sub)
        let ep2 = insertEpisode(guid: "iso-2", in: sub)

        store.toggleEpisodeStarred(ep1.id)

        XCTAssertTrue(store.episode(id: ep1.id)?.isStarred ?? false, "ep1 should be starred")
        XCTAssertFalse(store.episode(id: ep2.id)?.isStarred ?? true, "ep2 must be unaffected")
    }

    // MARK: - setEpisodeStarred

    func testSetEpisodeStarredTrueStarsEpisode() {
        let ep = insertEpisode(guid: "set-1")
        store.setEpisodeStarred(ep.id, true)
        XCTAssertTrue(store.episode(id: ep.id)?.isStarred ?? false)
    }

    func testSetEpisodeStarredFalseUnstarsEpisode() {
        let ep = insertEpisode(guid: "set-2")
        store.setEpisodeStarred(ep.id, true)
        XCTAssertTrue(store.episode(id: ep.id)?.isStarred ?? false, "pre-condition")

        store.setEpisodeStarred(ep.id, false)

        XCTAssertFalse(store.episode(id: ep.id)?.isStarred ?? true)
    }

    func testSetEpisodeStarredIsIdempotentWhenAlreadyTrue() {
        let ep = insertEpisode(guid: "idemp-t")
        store.setEpisodeStarred(ep.id, true)

        // The guard in setEpisodeStarred blocks redundant writes. We can't
        // observe the save count directly, but a second call with the same
        // value must leave the flag unchanged.
        store.setEpisodeStarred(ep.id, true)

        XCTAssertTrue(store.episode(id: ep.id)?.isStarred ?? false)
    }

    func testSetEpisodeStarredIsIdempotentWhenAlreadyFalse() {
        let ep = insertEpisode(guid: "idemp-f")
        store.setEpisodeStarred(ep.id, false)
        store.setEpisodeStarred(ep.id, false) // should not crash or flip
        XCTAssertFalse(store.episode(id: ep.id)?.isStarred ?? true)
    }

    func testSetEpisodeStarredIsNopForUnknownID() {
        store.setEpisodeStarred(UUID(), true) // must not crash
        XCTAssertTrue(store.state.episodes.allSatisfy { !$0.isStarred })
    }

    // MARK: - Episode insert invariant
    //
    // The RSS feed-refresh merge-preservation policy (same-guid upsert keeps
    // the user-mutable `isStarred` flag) was removed from Swift: RSS feeds are
    // ingested by the Rust kernel and `isStarred` round-trips through the
    // snapshot projection (`EpisodeSummary.toEpisode`), covered by
    // `cargo test -p nmp-app-podcast`. `upsertEpisodes` is now an INSERT-only
    // seam for agent-synthesized episodes, so the only invariant left to assert
    // here is that a freshly-inserted episode defaults to unstarred.

    func testNewEpisodeFromFeedDefaultsToUnstarred() {
        let sub = addSubscription(title: "NewFeed")
        store.upsertEpisodes([makeEpisode(podcastID: sub.id, guid: "brand-new")], forPodcast: sub.id)
        let stored = store.episodes(forPodcast: sub.id).first!
        XCTAssertFalse(stored.isStarred)
    }

    func testSetEpisodeStarredAgreesWithToggle() {
        let ep = insertEpisode(guid: "agree")
        store.setEpisodeStarred(ep.id, true)
        store.toggleEpisodeStarred(ep.id) // should flip back to false
        XCTAssertFalse(store.episode(id: ep.id)?.isStarred ?? true)

        store.toggleEpisodeStarred(ep.id) // flip back to true
        store.setEpisodeStarred(ep.id, false) // explicit false
        XCTAssertFalse(store.episode(id: ep.id)?.isStarred ?? true)
    }

    // MARK: - Fixtures

    @discardableResult
    private func addSubscription(title: String) -> Podcast {
        let sub = Podcast(
            feedURL: URL(string: "https://example.com/\(UUID().uuidString).xml")!,
            title: title
        )
        store.upsertPodcast(sub)
        store.addSubscription(podcastID: sub.id)
        return sub
    }

    @discardableResult
    private func insertEpisode(guid: String, in sub: Podcast? = nil) -> Episode {
        let podcast = sub ?? addSubscription(title: "Default-\(UUID().uuidString)")
        store.upsertEpisodes([makeEpisode(podcastID: podcast.id, guid: guid)], forPodcast: podcast.id)
        return store.episodes(forPodcast: podcast.id).first { $0.guid == guid }!
    }

    private func makeEpisode(podcastID: UUID, guid: String, title: String? = nil) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: guid,
            title: title ?? "Episode \(guid)",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }
}
