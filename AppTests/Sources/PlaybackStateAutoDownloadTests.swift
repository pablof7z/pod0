import Foundation
import XCTest
@testable import Podcastr

/// Verifies the playback boundary triggers a background download for any
/// episode the user streams that isn't yet on disk.
///
/// The download closure (`PlaybackState.onEnsureDownloadEnqueued`) is the
/// indirection point: `RootView` wires it to
/// `EpisodeDownloadService.ensureDownloadEnqueued`, tests stub it directly
/// so the URLSession / `AppStateStore` graph stays out of the test fixture.
@MainActor
final class PlaybackStateAutoDownloadTests: XCTestCase {

    func testNotDownloadedEpisodeFiresDownloadOnNewLoad() async {
        let episode = makeEpisode(downloadState: .notDownloaded)
        let fixture = makeFixture(episodes: [episode])
        defer { fixture.persistence.reset() }
        let requested = expectation(description: "download requested")
        var calls: [UUID] = []
        fixture.playback.onEnsureDownloadEnqueued = {
            calls.append($0)
            requested.fulfill()
        }

        fixture.playback.setEpisode(episode)

        await fulfillment(of: [requested], timeout: 1)
        XCTAssertEqual(calls, [episode.id])
    }

    func testSameEpisodeReloadDoesNotFireSecondDownload() async {
        // Play/Resume taps, deep-link replays, chapter-row taps all hit
        // `setEpisode` on every gesture. Re-firing the download trigger
        // would spam the queue / clobber resume data — verify the
        // same-episode reload path skips it.
        let played = expectation(description: "Rust committed playing")
        let episode = makeEpisode(downloadState: .notDownloaded)
        let fixture = makeFixture(
            episodes: [episode],
            productSignals: ProductSignalExpectationSink(
                name: .playStarted,
                expectation: played
            )
        )
        defer { fixture.persistence.reset() }
        let requested = expectation(description: "download requested once")
        var calls: [UUID] = []
        fixture.playback.onEnsureDownloadEnqueued = {
            calls.append($0)
            requested.fulfill()
        }

        fixture.playback.setEpisode(episode)
        await fulfillment(of: [requested], timeout: 1)
        fixture.playback.setEpisode(episode)
        fixture.playback.play()

        await fulfillment(of: [played], timeout: 1)
        XCTAssertEqual(calls, [episode.id])
    }

    func testDownloadedEpisodeDoesNotFireDownload() async {
        let played = expectation(description: "Rust committed playing")
        let episode = makeEpisode(
            downloadState: .downloaded(
                localFileURL: URL(fileURLWithPath: "/tmp/episode.mp3"),
                byteCount: 4096
            )
        )
        let fixture = makeFixture(
            episodes: [episode],
            productSignals: ProductSignalExpectationSink(
                name: .playStarted,
                expectation: played
            )
        )
        defer { fixture.persistence.reset() }
        var calls: [UUID] = []
        fixture.playback.onEnsureDownloadEnqueued = { calls.append($0) }

        fixture.playback.setEpisode(episode)
        fixture.playback.play()

        await fulfillment(of: [played], timeout: 1)
        XCTAssertTrue(calls.isEmpty)
    }

    func testNewEpisodeAfterDifferentEpisodeFiresDownloadForEachNotDownloaded() async {
        // Distinct from the same-episode-reload case: a brand-new
        // episode ID always re-evaluates `downloadState`, so playing two
        // different un-downloaded episodes in sequence should enqueue
        // both.
        let first = makeEpisode(downloadState: .notDownloaded)
        let second = makeEpisode(downloadState: .notDownloaded)
        let fixture = makeFixture(episodes: [first, second])
        defer { fixture.persistence.reset() }
        let requested = expectation(description: "both downloads requested")
        requested.expectedFulfillmentCount = 2
        var calls: [UUID] = []
        fixture.playback.onEnsureDownloadEnqueued = {
            calls.append($0)
            requested.fulfill()
        }

        fixture.playback.setEpisode(first)
        fixture.playback.setEpisode(second)

        await fulfillment(of: [requested], timeout: 1)
        XCTAssertEqual(calls, [first.id, second.id])
    }

    private func makeFixture(
        episodes: [Episode],
        productSignals: any ProductSignalSink = DiscardingProductSignalSink.shared
    ) -> (persistence: Persistence, store: AppStateStore, playback: PlaybackState) {
        let persistence = Persistence(fileURL: AppStateTestSupport.uniqueTempFileURL())
        var legacy = AppState()
        legacy.podcasts = episodes.map { episode in
            Podcast(
                id: episode.podcastID,
                feedURL: URL(string: "https://example.com/\(episode.podcastID).xml")!,
                title: "Show \(episode.podcastID)",
                discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
            )
        }
        legacy.episodes = episodes
        XCTAssertTrue(persistence.write(legacy, revision: 1))
        let store = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        let playback = PlaybackState(productSignals: productSignals)
        store.sharedLibrary?.attachPlayback(playback, store: store)
        XCTAssertNotNil(store.sharedLibrary)
        return (persistence, store, playback)
    }

    private func makeEpisode(
        id: UUID = UUID(),
        title: String = "Episode",
        duration: TimeInterval = 300,
        downloadState: DownloadState = .notDownloaded
    ) -> Episode {
        Episode(
            id: id,
            podcastID: UUID(),
            guid: "episode-\(id.uuidString)",
            title: title,
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            duration: duration,
            enclosureURL: URL(string: "https://example.com/\(id.uuidString).mp3")!,
            downloadState: downloadState
        )
    }
}
