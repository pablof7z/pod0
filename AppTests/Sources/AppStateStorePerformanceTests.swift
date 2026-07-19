import XCTest
@testable import Podcastr

/// Performance and invalidation coverage for native read projections built
/// from bounded Rust library snapshots. Policy and durable listening writes
/// are tested in the shared-core vertical-slice suites.
@MainActor
final class AppStateStorePerformanceTests: XCTestCase {

    private var fileURL: URL!
    var store: AppStateStore!
    var downloadEvidenceURLs: [URL] = []

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        fileURL = made.fileURL
        store = made.store
        downloadEvidenceURLs = []
    }

    override func tearDown() async throws {
        if let fileURL {
            AppStateTestSupport.disposeIsolatedStore(at: fileURL)
        }
        for url in downloadEvidenceURLs {
            try? FileManager.default.removeItem(at: url)
        }
        store = nil
        fileURL = nil
        try await super.tearDown()
    }

    // MARK: - Performance

    /// Exercises every hot projection against the same 10k-episode snapshot.
    /// Persisting that realistic fixture dominates test time, so sharing it
    /// keeps the suite fast without weakening any individual timing bound.
    func testProjectionReadsAreConstantTimeAtTenThousandEpisodes() {
        seedLargeState()
        let subs = store.state.subscriptions

        do {
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
                "20,000 unplayedCount lookups took \(elapsed)s — projection cache regressed."
            )
        }

        do {
            let start = Date()
            var hits = 0
            for _ in 0..<1_000 {
                for sub in subs where store.hasDownloadedEpisode(forPodcast: sub.id) {
                    hits += 1
                }
            }
            let elapsed = Date().timeIntervalSince(start)

            XCTAssertGreaterThan(hits, 0, "Sanity: at least one downloaded episode in seed.")
            XCTAssertLessThan(elapsed, 0.05, "Downloaded projection lookups took \(elapsed)s.")
        }

        do {
            let start = Date()
            var hits = 0
            for _ in 0..<1_000 {
                for sub in subs where store.hasTranscribedEpisode(forPodcast: sub.id) {
                    hits += 1
                }
            }
            let elapsed = Date().timeIntervalSince(start)

            XCTAssertGreaterThan(hits, 0, "Sanity: at least one transcribed episode in seed.")
            XCTAssertLessThan(elapsed, 0.05, "Transcript projection lookups took \(elapsed)s.")
        }

        do {
            let largest = largestSubscriptionByEpisodeCount()
            let start = Date()
            var totalReturned = 0
            for _ in 0..<100 {
                totalReturned += store.episodes(forPodcast: largest.id).count
            }
            let elapsed = Date().timeIntervalSince(start)

            XCTAssertGreaterThan(totalReturned, 0)
            XCTAssertLessThan(elapsed, 0.1, "Episode slice lookups took \(elapsed)s.")
        }

        do {
            let start = Date()
            var hits = 0
            for _ in 0..<1_000 {
                hits += store.inProgressEpisodes.count
                hits += store.recentEpisodes(limit: 30).count
            }
            let elapsed = Date().timeIntervalSince(start)

            XCTAssertGreaterThan(hits, 0)
            XCTAssertLessThan(elapsed, 0.1, "Home feed projection reads took \(elapsed)s.")
        }
    }

    // MARK: - Correctness: invalidation

    func testUpsertEpisodesAddsToUnplayedCount() {
        let sub = addSubscription(title: "Upsert")
        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 0)

        store.installEpisodeFixtures(
            [makeEpisode(podcastID: sub.id, guid: "u1"),
             makeEpisode(podcastID: sub.id, guid: "u2")],
            forPodcast: sub.id
        )

        XCTAssertEqual(store.unplayedCount(forPodcast: sub.id), 2)
    }

    func testSetDownloadStateUpdatesHasDownloadedSet() throws {
        let sub = addSubscription(title: "Download")
        let ep = makeEpisode(podcastID: sub.id, guid: "d1")
        store.installEpisodeFixtures([ep], forPodcast: sub.id)
        XCTAssertFalse(store.hasDownloadedEpisode(forPodcast: sub.id))

        let fileURL = try installDownloadEvidence(for: ep)
        store.setEpisodeDownloadState(
            ep.id,
            state: .downloaded(localFileURL: fileURL, byteCount: 100)
        )
        XCTAssertTrue(store.hasDownloadedEpisode(forPodcast: sub.id))

        store.setEpisodeDownloadState(ep.id, state: .notDownloaded)
        XCTAssertFalse(store.hasDownloadedEpisode(forPodcast: sub.id))
    }

    func testCommittedTranscriptUpdatesHasTranscribedSet() async throws {
        let sub = addSubscription(title: "Transcript")
        let ep = try await store.upsertExternalEpisodeAndWait(
            podcastID: sub.id,
            feedURL: sub.feedURL,
            podcastTitle: sub.title,
            audioURL: URL(string: "https://example.com/t1.mp3")!,
            title: "Episode t1",
            imageURL: nil,
            duration: 60
        )
        XCTAssertFalse(store.hasTranscribedEpisode(forPodcast: sub.id))

        try installTranscriptEvidence(for: ep, source: .scribeV1)
        XCTAssertTrue(store.hasTranscribedEpisode(forPodcast: sub.id))
    }

    func testEpisodesForSubscriptionStaysSortedNewestFirst() {
        let sub = addSubscription(title: "Sorted")
        let now = Date()
        var older = makeEpisode(podcastID: sub.id, guid: "old")
        older.pubDate = now.addingTimeInterval(-86_400)
        var newer = makeEpisode(podcastID: sub.id, guid: "new")
        newer.pubDate = now
        store.installEpisodeFixtures([older, newer], forPodcast: sub.id)

        let listed = store.episodes(forPodcast: sub.id)
        XCTAssertEqual(listed.map(\.guid), ["new", "old"])
    }

    func testRecentEpisodesReadsFromCacheAndStripsPlayed() {
        let sub = addSubscription(title: "Recent")
        let unplayed = makeEpisode(podcastID: sub.id, guid: "rec-u")
        var played = makeEpisode(podcastID: sub.id, guid: "rec-p")
        played.played = true
        store.installEpisodeFixtures([unplayed, played], forPodcast: sub.id)

        let listed = store.recentEpisodes(limit: 30)
        XCTAssertEqual(listed.count, 1)
        XCTAssertEqual(listed.first?.id, unplayed.id)
    }

    // MARK: - Fixtures

    @discardableResult
    private func addSubscription(title: String) -> Podcast {
        let sub = Podcast(
            feedURL: URL(string: "https://example.com/\(UUID().uuidString).xml")!,
            title: title
        )
        store.installPodcastFixture(sub)
        store.installSubscriptionFixture(podcastID: sub.id)
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

        // Fixture construction is not the behavior measured by these tests.
        // Assign one complete snapshot so neither AppState copy-on-write nor
        // the authoritative SQLite backend processes 40 growing intermediates.
        let episodes = subs.flatMap { episodesBySub[$0.id] ?? [] }
        store.mutateState { state in
            state.podcasts = subs
            state.subscriptions = subs.map { PodcastSubscription(podcastID: $0.id) }
            state.episodes = episodes
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
