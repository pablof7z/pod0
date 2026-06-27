import XCTest
@testable import Podcastr

/// Coverage for in-app and remote transport control dispatch order.
///
/// All transport commands must dispatch to Rust before native engines
/// execute, ensuring Rust is the authoritative playback policy source.
///
/// `AppStateStore` is a `final` class with kernel methods declared in
/// extensions, so it cannot be subclassed in tests. `PlaybackState` exposes
/// a `kernelDispatch: (any KernelPlaybackDispatching)?` seam that, when set,
/// takes precedence over `store`. Tests inject `StubKernelTransport` via
/// that seam without touching the live store.
@MainActor
final class PlaybackTransportRoutingTests: XCTestCase {

    private var playbackState: PlaybackState!
    private var stub: StubKernelTransport!

    override func setUp() async throws {
        playbackState = PlaybackState()
        stub = StubKernelTransport()
        playbackState.kernelDispatch = stub
    }

    override func tearDown() async throws {
        playbackState.kernelDispatch = nil
        playbackState = nil
        stub = nil
    }

    // MARK: - Tests

    func testSetEpisodeDispatchesKernelLoadForNewEpisode() {
        let episode = Episode(
            id: UUID(),
            podcastID: UUID(),
            guid: "test-guid-1",
            title: "Test Episode",
            pubDate: Date(),
            duration: 3600,
            enclosureURL: URL(fileURLWithPath: "/test.mp3")
        )
        playbackState.setEpisode(episode, playAfterLoad: false)

        XCTAssertEqual(stub.kernelLoadCallCount, 1)
        XCTAssertEqual(stub.lastLoadedEpisodeID, episode.id)
    }

    func testPlayDispatchesKernelResume() {
        let episode = Episode(
            id: UUID(),
            podcastID: UUID(),
            guid: "test-guid-2",
            title: "Test Episode",
            pubDate: Date(),
            duration: 3600,
            enclosureURL: URL(fileURLWithPath: "/test.mp3")
        )
        playbackState.setEpisode(episode, playAfterLoad: false)

        playbackState.play()

        XCTAssertEqual(stub.kernelResumeCallCount, 1)
    }

    func testPauseDispatchesKernelPause() {
        stub.reset()

        playbackState.pause()

        XCTAssertEqual(stub.kernelPauseCallCount, 1)
    }

    func testSeekDispatchesKernelSeekBeforeEngineSeek() {
        stub.reset()

        let episode = Episode(
            id: UUID(),
            podcastID: UUID(),
            guid: "test-guid-3",
            title: "Test Episode",
            pubDate: Date(),
            duration: 3600,
            enclosureURL: URL(fileURLWithPath: "/test.mp3")
        )
        playbackState.setEpisode(episode, playAfterLoad: false)
        playbackState.seek(to: 100)

        XCTAssertEqual(stub.kernelSeekCallCount, 1)
        XCTAssertEqual(stub.lastSeekPosition, 100)
    }

    func testSkipForwardDispatchesKernelSkipForward() {
        stub.reset()

        playbackState.skipForward(15)

        XCTAssertEqual(stub.kernelSkipForwardCallCount, 1)
    }

    func testSkipBackwardDispatchesKernelSkipBackward() {
        stub.reset()

        playbackState.skipBackward(15)

        XCTAssertEqual(stub.kernelSkipBackwardCallCount, 1)
    }

    func testSetRateDispatchesKernelSetSpeed() {
        stub.reset()

        playbackState.setRate(.fast)

        XCTAssertEqual(stub.kernelSetSpeedCallCount, 1)
        XCTAssertEqual(stub.lastSetSpeedValue, 1.5)
    }

    func testSetRateAppliesAcceptedKernelSetSpeed() {
        stub.reset()
        stub.setSpeedResult = .accepted(correlationId: "speed-accepted")
        playbackState.engine.setRate(1.0)

        playbackState.setRate(.fast)

        XCTAssertEqual(playbackState.engine.rate, 1.5)
    }

    func testSetRateDoesNotApplyFailedKernelSetSpeed() {
        stub.reset()
        stub.setSpeedResult = .failure("rejected")
        playbackState.engine.setRate(1.0)

        playbackState.setRate(.fast)

        XCTAssertEqual(playbackState.engine.rate, 1.0)
    }
}

// MARK: - Stub

/// Lightweight recording stub conforming to `KernelPlaybackDispatching`.
/// Does not subclass or instantiate `AppStateStore` — safe to use in JVM
/// unit tests and in XCTest without a live kernel bridge.
@MainActor
final class StubKernelTransport: KernelPlaybackDispatching {

    var kernelLoadCallCount = 0
    var lastLoadedEpisodeID: UUID?
    var kernelPlayCallCount = 0
    var lastPlayedEpisodeID: UUID?
    var kernelResumeCallCount = 0
    var kernelPauseCallCount = 0
    var kernelSeekCallCount = 0
    var kernelSkipForwardCallCount = 0
    var kernelSkipBackwardCallCount = 0
    var kernelSetSpeedCallCount = 0
    var setSpeedResult: DispatchResult? = .accepted(correlationId: "stub-set-speed")

    var lastSeekPosition: Double = 0
    var lastSetSpeedValue: Double = 0

    func reset() {
        kernelLoadCallCount = 0
        lastLoadedEpisodeID = nil
        kernelPlayCallCount = 0
        lastPlayedEpisodeID = nil
        kernelResumeCallCount = 0
        kernelPauseCallCount = 0
        kernelSeekCallCount = 0
        kernelSkipForwardCallCount = 0
        kernelSkipBackwardCallCount = 0
        kernelSetSpeedCallCount = 0
        setSpeedResult = .accepted(correlationId: "stub-set-speed")
        lastSeekPosition = 0
        lastSetSpeedValue = 0
    }

    func kernelLoad(episodeID: UUID) {
        kernelLoadCallCount += 1
        lastLoadedEpisodeID = episodeID
    }

    func kernelPlay(episodeID: UUID, startSeconds: Double?, endSeconds: Double?) -> DispatchResult? {
        kernelPlayCallCount += 1
        lastPlayedEpisodeID = episodeID
        return nil
    }

    func kernelResume() {
        kernelResumeCallCount += 1
    }

    func kernelPause() -> DispatchResult? {
        kernelPauseCallCount += 1
        return nil
    }

    func kernelSeek(positionSecs: Double) -> DispatchResult? {
        kernelSeekCallCount += 1
        lastSeekPosition = positionSecs
        return nil
    }

    func kernelSkipForward(secs: Double?) -> DispatchResult? {
        kernelSkipForwardCallCount += 1
        return nil
    }

    func kernelSkipBackward(secs: Double?) -> DispatchResult? {
        kernelSkipBackwardCallCount += 1
        return nil
    }

    func kernelSetSpeed(_ speed: Double) -> DispatchResult? {
        kernelSetSpeedCallCount += 1
        lastSetSpeedValue = speed
        return setSpeedResult
    }
}
