import XCTest
@testable import Podcastr

/// Coverage for the `AppStateStore` episode-projection cache that backs
/// `unplayedCount(forPodcast:)`, `hasDownloadedEpisode(forPodcast:)`,
/// `hasTranscribedEpisode(forPodcast:)`, `episodes(forPodcast:)`,
/// `inProgressEpisodes`, and `recentEpisodes(limit:)`.
///
/// **Why the cache exists.** Each of those reads used to scan the full
/// `state.episodes` array on every call, and SwiftUI calls them from grid-
/// cell `body` getters that fire on every scroll tick. With 20 subscriptions
/// and 10k episodes, scrolling Library ran 20 × 10k = 200k filter iterations
/// per frame. `sample` showed `unplayedCount(forPodcast:)` at 27 ticks
/// per second of scrolling, dominated by `Episode` struct copies. After the
/// projection landed, the same call is an O(1) dict lookup.
///
/// **What this file tests.**
///   - Performance: a 10k-episode / 20-subscription state, 1000 calls to
///     each cached helper, asserting an absolute upper bound that no
///     reasonable hardware will violate but that the pre-fix code would
///     blow past by 100×.
///   - Correctness: every documented invalidation path (upsert, mark
///     played / unplayed, set download state, set transcript state, remove
///     subscription, position-cache flush, `clearAllData`) drops or adds
///     the right entries from the cache. A perf regression that silently
///     stale-caches would be invisible to the perf assertions but would
///     show stale unplayed dots in the UI.
@MainActor
final class AppStateStorePerformanceTests: XCTestCase {

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

    // MARK: - Performance

    /// 1000 `unplayedCount(forPodcast:)` calls against a 20-sub /
    /// 10k-episode state must complete in well under 50 ms. The pre-fix
    /// reduce was ~10 µs per call (10k iterations + closure dispatch),
    /// so 1000 calls × 20 subs spent ~200 ms in user time alone — a
    /// scroll-rate problem. Post-fix is a dict lookup, ~50 ns per call.
    func testUnplayedCountIsConstantTime() {
        seedLargeState()
        let subs = store.state.subscriptions

        let start = Date()
        var total = 0
        for _ in 0..<1_000 {
            for sub in subs {
                total += store.unplayedCount(forPodcast: sub.id)
            }
        }
        let elapsed = Date().timeIntervalSince(start)

        XCTAssertGreaterThan(total, 0, "Sanity: at least one unplayed episode in seed.")
        XCTAssertLessThan(
            elapsed, 0.05,
            "1000 × 20 = 20,000 unplayedCount lookups took \(elapsed)s — projection cache regressed to a per-call O(N) scan."
        )
    }

    /// `hasDownloadedEpisode` should be a Set lookup. Same cap as the
    /// unplayed count test.
    func testHasDownloadedEpisodeIsConstantTime() {
        seedLargeState()
        let subs = store.state.subscriptions

        let start = Date()
        var hits = 0
        for _ in 0..<1_000 {
            for sub in subs where store.hasDownloadedEpisode(forPodcast: sub.id) {
                hits += 1
            }
        }
        let elapsed = Date().timeIntervalSince(start)

        XCTAssertGreaterThan(hits, 0, "Sanity: at least one downloaded episode in seed.")
        XCTAssertLessThan(
            elapsed, 0.05,
            "1000 × 20 hasDownloadedEpisode lookups took \(elapsed)s — Set membership regressed."
        )
    }

    /// `hasTranscribedEpisode` should be a Set lookup. Same cap as above.
    func testHasTranscribedEpisodeIsConstantTime() {
        seedLargeState()
        let subs = store.state.subscriptions

        let start = Date()
        var hits = 0
        for _ in 0..<1_000 {
            for sub in subs where store.hasTranscribedEpisode(forPodcast: sub.id) {
                hits += 1
            }
        }
        let elapsed = Date().timeIntervalSince(start)

        XCTAssertGreaterThan(hits, 0, "Sanity: at least one transcribed episode in seed.")
        XCTAssertLessThan(
            elapsed, 0.05,
            "1000 × 20 hasTranscribedEpisode lookups took \(elapsed)s — Set membership regressed."
        )
    }

    /// `episodes(forPodcast:)` returns a precomputed slice. 100 calls
    /// against the largest seeded show (~500 episodes) must complete in
    /// well under 100 ms — the pre-fix filter+sort path took ~5 ms each
    /// for that show, so 100 calls = 500 ms.
    func testEpisodesForSubscriptionIsCheapPerCall() {
        seedLargeState()
        let largest = largestSubscriptionByEpisodeCount()

        let start = Date()
        var totalReturned = 0
        for _ in 0..<100 {
            totalReturned += store.episodes(forPodcast: largest.id).count
        }
        let elapsed = Date().timeIntervalSince(start)

        XCTAssertGreaterThan(totalReturned, 0)
        XCTAssertLessThan(
            elapsed, 0.1,
            "100 episodes(forPodcast:) calls took \(elapsed)s — projection cache regressed to a per-call filter+sort."
        )
    }

    /// `inProgressEpisodes` and `recentEpisodes(limit:)` are read on every
    /// HomeView render. They must be cheap.
    func testHomeFeedsAreCheapPerCall() {
        seedLargeState()

        let start = Date()
        var hits = 0
        for _ in 0..<1_000 {
            hits += store.inProgressEpisodes.count
            hits += store.recentEpisodes(limit: 30).count
        }
        let elapsed = Date().timeIntervalSince(start)

        XCTAssertGreaterThan(hits, 0)
        XCTAssertLessThan(
            elapsed, 0.1,
            "1000 × (inProgressEpisodes + recentEpisodes) took \(elapsed)s — Home feed cache regressed."
        )
    }

    func testBatchSubscriptionImportPersistsOnce() {
        let payloads = (0..<5).map { i in
            makeImportPayload(index: i, episodeCount: 25)
        }

        store.persistence.resetSaveInvocationCount()
        let result = store.addSubscriptions(payloads)

        XCTAssertEqual(result, SubscriptionImportResult(imported: 5, skipped: 0))
        XCTAssertEqual(store.state.subscriptions.count, 5)
        XCTAssertEqual(store.state.episodes.count, 125)
        XCTAssertEqual(
            store.persistence.saveInvocationCount,
            1,
            "Batch import should perform one state save, not one save per subscription or episode batch."
        )
        XCTAssertEqual(store.unplayedCount(forPodcast: payloads[0].podcast.id), 25)
    }

    // MARK: - Correctness: invalidation

    func testUpsertEpisodesAddsToUnplayedCount() {
        let sub = addSubscription(title: "Upsert")
        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 0)

        store.upsertEpisodes(
            [makeEpisode(podcastID: sub.id, guid: "u1"),
             makeEpisode(podcastID: sub.id, guid: "u2")],
            forPodcast: sub.id
        )

        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 2)
    }

    func testMarkPlayedDecrementsUnplayedCount() {
        let sub = addSubscription(title: "Played")
        let ep = makeEpisode(podcastID: sub.id, guid: "p1")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 1)

        store.markEpisodePlayed(ep.id)

        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 0)
    }

    func testMarkUnplayedRestoresUnplayedCount() {
        let sub = addSubscription(title: "Unplayed")
        let ep = makeEpisode(podcastID: sub.id, guid: "u1")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        store.markEpisodePlayed(ep.id)
        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 0)

        store.markEpisodeUnplayed(ep.id)

        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 1)
    }

    func testSetDownloadStateUpdatesHasDownloadedSet() {
        let sub = addSubscription(title: "Download")
        let ep = makeEpisode(podcastID: sub.id, guid: "d1")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        XCTAssertFalse(store.hasDownloadedEpisode(forPodcast: sub.id))

        store.setEpisodeDownloadState(
            ep.id,
            state: .downloaded(localFileURL: URL(fileURLWithPath: "/tmp/x.mp3"), byteCount: 100)
        )
        XCTAssertTrue(store.hasDownloadedEpisode(forPodcast: sub.id))

        store.setEpisodeDownloadState(ep.id, state: .notDownloaded)
        XCTAssertFalse(store.hasDownloadedEpisode(forPodcast: sub.id))
    }

    func testSetTranscriptStateUpdatesHasTranscribedSet() {
        let sub = addSubscription(title: "Transcript")
        let ep = makeEpisode(podcastID: sub.id, guid: "t1")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        XCTAssertFalse(store.hasTranscribedEpisode(forPodcast: sub.id))

        store.setEpisodeTranscriptState(ep.id, state: .ready(source: .scribe))
        XCTAssertTrue(store.hasTranscribedEpisode(forPodcast: sub.id))

        store.setEpisodeTranscriptState(ep.id, state: .none)
        XCTAssertFalse(store.hasTranscribedEpisode(forPodcast: sub.id))
    }

    func testRemoveSubscriptionEvictsFromAllProjections() {
        let sub = addSubscription(title: "Evict")
        let ep = makeEpisode(podcastID: sub.id, guid: "ev1")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        store.setEpisodeDownloadState(
            ep.id,
            state: .downloaded(localFileURL: URL(fileURLWithPath: "/tmp/x.mp3"), byteCount: 1)
        )
        store.setEpisodeTranscriptState(ep.id, state: .ready(source: .publisher))
        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 1)
        XCTAssertTrue(store.hasDownloadedEpisode(forPodcast: sub.id))
        XCTAssertTrue(store.hasTranscribedEpisode(forPodcast: sub.id))
        XCTAssertFalse(store.episodes(forPodcast: sub.id).isEmpty)

        store.deletePodcast(podcastID: sub.id)

        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 0)
        XCTAssertFalse(store.hasDownloadedEpisode(forPodcast: sub.id))
        XCTAssertFalse(store.hasTranscribedEpisode(forPodcast: sub.id))
        XCTAssertTrue(store.episodes(forPodcast: sub.id).isEmpty)
    }

    func testEpisodesForSubscriptionStaysSortedNewestFirst() {
        let sub = addSubscription(title: "Sorted")
        let now = Date()
        var older = makeEpisode(podcastID: sub.id, guid: "old")
        older.pubDate = now.addingTimeInterval(-86_400)
        var newer = makeEpisode(podcastID: sub.id, guid: "new")
        newer.pubDate = now
        store.upsertEpisodes([older, newer], forPodcast: sub.id)

        let listed = store.episodes(forPodcast: sub.id)
        XCTAssertEqual(listed.map(\.guid), ["new", "old"])
    }

    func testStartingAnEpisodeSurfacesItInInProgressBeforeFlush() {
        // Position cache fold at read time must surface an episode whose
        // persisted position is still 0 but whose cache has crossed > 0.
        let sub = addSubscription(title: "InProg")
        let ep = makeEpisode(podcastID: sub.id, guid: "ip-cache")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        XCTAssertTrue(store.inProgressEpisodes.isEmpty)

        store.setEpisodePlaybackPosition(ep.id, position: 42)

        // Eager-first flushes the first call, so the projection now lists
        // the episode either way. Force the cache-only path explicitly:
        store.setEpisodePlaybackPosition(ep.id, position: 99)

        let listed = store.inProgressEpisodes
        XCTAssertEqual(listed.first?.id, ep.id)
        XCTAssertEqual(listed.first?.playbackPosition ?? 0, 99, accuracy: 0.001)
    }

    /// Regression: a cached `positionCache[id] == 0` (engine wrote a zero
    /// after a non-zero — e.g. user scrubbed back to the very start) must
    /// remove the episode from `inProgressEpisodes`. Without the
    /// post-fold `> 0` filter, the position-cache fold would overwrite
    /// the cached projection's non-zero value with 0 but leave the
    /// episode in the result list, leaving a phantom Continue Listening
    /// entry until the next flush.
    func testInProgressDropsEpisodeWhenCachedPositionIsZero() {
        let sub = addSubscription(title: "InProgZero")
        let ep = makeEpisode(podcastID: sub.id, guid: "ip-zero")
        store.upsertEpisodes([ep], forPodcast: sub.id)

        // First call eagerly persists position 50, so the projection
        // includes the episode with persisted = 50.
        store.setEpisodePlaybackPosition(ep.id, position: 50)
        XCTAssertEqual(store.inProgressEpisodes.count, 1)

        // Second call lands in the cache only (within the eager-cap and
        // debounce windows), overwriting the cached value to 0. The
        // post-fold filter must drop the episode from the rail.
        store.setEpisodePlaybackPosition(ep.id, position: 0)

        XCTAssertTrue(
            store.inProgressEpisodes.isEmpty,
            "Cached position == 0 must evict the episode from the in-progress rail."
        )
    }

    func testRecentEpisodesReadsFromCacheAndStripsPlayed() {
        let sub = addSubscription(title: "Recent")
        let unplayed = makeEpisode(podcastID: sub.id, guid: "rec-u")
        let played = makeEpisode(podcastID: sub.id, guid: "rec-p")
        store.upsertEpisodes([unplayed, played], forPodcast: sub.id)
        store.markEpisodePlayed(played.id)

        let listed = store.recentEpisodes(limit: 30)
        XCTAssertEqual(listed.count, 1)
        XCTAssertEqual(listed.first?.id, unplayed.id)
    }

    func testClearAllDataEmptiesProjections() {
        let sub = addSubscription(title: "Wipe")
        let ep = makeEpisode(podcastID: sub.id, guid: "wipe-1")
        store.upsertEpisodes([ep], forPodcast: sub.id)
        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 1)

        store.clearAllData()

        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 0)
        XCTAssertTrue(store.inProgressEpisodes.isEmpty)
        XCTAssertTrue(store.recentEpisodes(limit: 30).isEmpty)
        XCTAssertTrue(store.episodes(forPodcast: sub.id).isEmpty)
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

    private func makeEpisode(podcastID: UUID, guid: String) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: guid,
            title: "Episode \(guid)",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }

    private func makeImportPayload(index: Int, episodeCount: Int) -> SubscriptionImportPayload {
        let sub = Podcast(
            feedURL: URL(string: "https://example.com/import-\(index).xml")!,
            title: "Import Show \(index)"
        )
        let episodes = (0..<episodeCount).map { episodeIndex in
            makeEpisode(podcastID: sub.id, guid: "import-\(index)-\(episodeIndex)")
        }
        return SubscriptionImportPayload(
            podcast: sub,
            subscription: PodcastSubscription(podcastID: sub.id),
            episodes: episodes
        )
    }

    /// Builds a state with 20 subscriptions and 10,000 episodes, mirroring
    /// the seeded persistence file the perf brief targets. Distribution
    /// across shows is intentionally non-uniform so the largest show has
    /// ~500 episodes — close to the 2,853-episode "The Daily" the brief
    /// flags as the worst-case ShowDetail render.
    private func seedLargeState() {
        let subs = (0..<20).map { i in
            Podcast(
                feedURL: URL(string: "https://example.com/seed-\(i).xml")!,
                title: "Seed Show \(i)"
            )
        }
        for sub in subs {
            store.upsertPodcast(sub)
            store.addSubscription(podcastID: sub.id)
        }

        let now = Date()
        // Spread 10,000 episodes across 20 shows. Use a deterministic
        // round-robin so the largest bucket is predictable for the
        // `episodes(forPodcast:)` perf test below.
        var episodesBySub: [UUID: [Episode]] = [:]
        for i in 0..<10_000 {
            let subID = subs[i % subs.count].id
            var ep = Episode(
                podcastID: subID,
                guid: "seed-\(i)",
                title: "Seed Episode \(i)",
                pubDate: now.addingTimeInterval(-Double(i) * 60),
                enclosureURL: URL(string: "https://example.com/seed-\(i).mp3")!
            )
            // Sprinkle some played / downloaded / transcribed episodes
            // across the seed so the cache has actual content beyond the
            // unplayed-only baseline.
            if i % 3 == 0 { ep.played = true }
            if i % 5 == 0 {
                ep.downloadState = .downloaded(
                    localFileURL: URL(fileURLWithPath: "/tmp/seed-\(i).mp3"),
                    byteCount: 1024
                )
            }
            if i % 7 == 0 {
                ep.transcriptState = .ready(source: .publisher)
            }
            episodesBySub[subID, default: []].append(ep)
        }

        for (subID, eps) in episodesBySub {
            store.upsertEpisodes(eps, forPodcast: subID)
        }
    }

    private func largestSubscriptionByEpisodeCount() -> PodcastSubscription {
        let subs = store.state.subscriptions
        var bestID = subs[0].id
        var bestCount = -1
        for sub in subs {
            let c = store.episodes(forPodcast: sub.id).count
            if c > bestCount {
                bestCount = c
                bestID = sub.id
            }
        }
        return subs.first { $0.id == bestID }!
    }
}
