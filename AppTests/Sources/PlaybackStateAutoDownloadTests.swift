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

    func testNotDownloadedEpisodeFiresDownloadOnNewLoad() {
        let state = PlaybackState()
        var calls: [UUID] = []
        state.onEnsureDownloadEnqueued = { calls.append($0) }

        let episode = makeEpisode(downloadState: .notDownloaded)
        state.setEpisode(episode)

        XCTAssertEqual(calls, [episode.id])
    }

    func testFailedEpisodeFiresDownloadOnNewLoad() {
        // `.failed` is the user's only re-try path short of opening the
        // diagnostics view — playback must re-enqueue it the same as
        // `.notDownloaded`.
        let state = PlaybackState()
        var calls: [UUID] = []
        state.onEnsureDownloadEnqueued = { calls.append($0) }

        let episode = makeEpisode(downloadState: .failed(message: "previous error"))
        state.setEpisode(episode)

        XCTAssertEqual(calls, [episode.id])
    }

    func testSameEpisodeReloadDoesNotFireSecondDownload() {
        // Play/Resume taps, deep-link replays, chapter-row taps all hit
        // `setEpisode` on every gesture. Re-firing the download trigger
        // would spam the queue / clobber resume data — verify the
        // same-episode reload path skips it.
        let state = PlaybackState()
        var calls: [UUID] = []
        state.onEnsureDownloadEnqueued = { calls.append($0) }

        let episode = makeEpisode(downloadState: .notDownloaded)
        state.setEpisode(episode)
        state.setEpisode(episode)

        XCTAssertEqual(calls, [episode.id])
    }

    func testDownloadedEpisodeDoesNotFireDownload() {
        let state = PlaybackState()
        var calls: [UUID] = []
        state.onEnsureDownloadEnqueued = { calls.append($0) }

        let local = URL(fileURLWithPath: "/tmp/episode.mp3")
        let episode = makeEpisode(
            downloadState: .downloaded(localFileURL: local, byteCount: 4096)
        )
        state.setEpisode(episode)

        XCTAssertTrue(calls.isEmpty)
    }

    func testDownloadingEpisodeDoesNotFireDownload() {
        let state = PlaybackState()
        var calls: [UUID] = []
        state.onEnsureDownloadEnqueued = { calls.append($0) }

        let episode = makeEpisode(
            downloadState: .downloading(progress: 0.42, bytesWritten: 1024)
        )
        state.setEpisode(episode)

        XCTAssertTrue(calls.isEmpty)
    }

    func testQueuedEpisodeDoesNotFireDownload() {
        // `.queued` means the auto-download policy already scheduled it
        // for the next Wi-Fi window — re-enqueueing on playback would
        // bypass the policy the user explicitly opted into.
        let state = PlaybackState()
        var calls: [UUID] = []
        state.onEnsureDownloadEnqueued = { calls.append($0) }

        let episode = makeEpisode(downloadState: .queued)
        state.setEpisode(episode)

        XCTAssertTrue(calls.isEmpty)
    }

    func testNewEpisodeAfterDifferentEpisodeFiresDownloadForEachNotDownloaded() {
        // Distinct from the same-episode-reload case: a brand-new
        // episode ID always re-evaluates `downloadState`, so playing two
        // different un-downloaded episodes in sequence should enqueue
        // both.
        let state = PlaybackState()
        var calls: [UUID] = []
        state.onEnsureDownloadEnqueued = { calls.append($0) }

        let first = makeEpisode(downloadState: .notDownloaded)
        let second = makeEpisode(downloadState: .notDownloaded)
        state.setEpisode(first)
        state.setEpisode(second)

        XCTAssertEqual(calls, [first.id, second.id])
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
            pubDate: Date(),
            duration: duration,
            enclosureURL: URL(string: "https://example.com/\(id.uuidString).mp3")!,
            downloadState: downloadState
        )
    }
}
