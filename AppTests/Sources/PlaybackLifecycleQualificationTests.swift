import AVFoundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class PlaybackLifecycleQualificationTests: XCTestCase {
    func testInterruptionPausesAndPersistsLatestPlayhead() {
        let state = PlaybackState()
        let episode = makeEpisode()
        var persisted: [(UUID, TimeInterval)] = []
        var flushCount = 0
        state.onPersistPosition = { persisted.append(($0, $1)) }
        state.onFlushPositions = { flushCount += 1 }
        state.setEpisode(episode)
        state.playbackRequested = true
        state.engine.setCurrentTime(84.25)
        state.engine.setState(.playing)
        let observedAt = Date(timeIntervalSince1970: 1_234)

        state.handleAudioSessionEvent(
            .interruptionBegan(route: .bluetooth),
            observedAt: observedAt
        )

        XCTAssertEqual(state.engine.state, .paused)
        XCTAssertEqual(persisted.count, 1)
        XCTAssertEqual(persisted.first?.0, episode.id)
        XCTAssertEqual(persisted.first?.1 ?? 0, 84.25, accuracy: 0.001)
        XCTAssertEqual(flushCount, 2, "Episode load and interruption must each create a durability boundary")
        XCTAssertEqual(
            state.lastHostObservation,
            PlaybackLifecycleObservation(
                episodeId: EpisodeId(uuid: episode.id),
                state: .paused,
                positionMilliseconds: 84_250,
                durationMilliseconds: 600_000,
                route: .bluetooth,
                interruption: .began,
                ended: false
            )
        )
    }

    func testInterruptionResumeRequiresSameEpisodeAndOSPermission() {
        let first = makeEpisode()
        let second = makeEpisode()
        var policy = PlaybackSessionPolicy()

        XCTAssertEqual(
            policy.handle(
                .interruptionBegan(route: .wired),
                episodeID: first.id,
                playbackRequested: true,
                didReachNaturalEnd: false
            ),
            .pauseAndPersist
        )
        XCTAssertEqual(
            policy.handle(
                .interruptionEnded(shouldResume: true, route: .wired),
                episodeID: second.id,
                playbackRequested: true,
                didReachNaturalEnd: false
            ),
            .none,
            "A stale interruption callback must never start the next queued episode"
        )

        _ = policy.handle(
            .interruptionBegan(route: .wired),
            episodeID: first.id,
            playbackRequested: true,
            didReachNaturalEnd: false
        )
        XCTAssertEqual(
            policy.handle(
                .interruptionEnded(shouldResume: false, route: .wired),
                episodeID: first.id,
                playbackRequested: true,
                didReachNaturalEnd: false
            ),
            .none
        )
    }

    func testMatchingInterruptionResumesRequestedEpisode() {
        let state = PlaybackState()
        let episode = makeEpisode()
        state.setEpisode(episode)
        state.playbackRequested = true
        state.engine.setState(.playing)
        state.handleAudioSessionEvent(.interruptionBegan(route: .builtIn))

        state.handleAudioSessionEvent(
            .interruptionEnded(shouldResume: true, route: .builtIn)
        )

        XCTAssertTrue(state.playbackRequested)
        XCTAssertTrue(state.isPlaying)
        XCTAssertEqual(state.episode?.id, episode.id)
    }

    func testInterruptionWithoutResumePermissionClearsPlaybackIntent() {
        let state = PlaybackState()
        let episode = makeEpisode()
        state.setEpisode(episode)
        state.playbackRequested = true
        state.engine.setState(.playing)
        state.handleAudioSessionEvent(.interruptionBegan(route: .builtIn))

        state.handleAudioSessionEvent(
            .interruptionEnded(shouldResume: false, route: .builtIn)
        )

        XCTAssertFalse(state.playbackRequested)
        XCTAssertFalse(state.isPlaying)
    }

    func testHeadphoneDisconnectAlwaysPausesAndClearsResumeIntent() {
        let episode = makeEpisode()
        var policy = PlaybackSessionPolicy()
        _ = policy.handle(
            .interruptionBegan(route: .bluetooth),
            episodeID: episode.id,
            playbackRequested: true,
            didReachNaturalEnd: false
        )

        XCTAssertEqual(
            policy.handle(
                .routeChanged(
                    reason: .oldDeviceUnavailable,
                    previous: .bluetooth,
                    current: .builtIn
                ),
                episodeID: episode.id,
                playbackRequested: true,
                didReachNaturalEnd: false
            ),
            .pauseAndPersist
        )
        XCTAssertNil(policy.interruptedEpisodeID)
    }

    func testMediaServicesResetRequestsAPlayerRebuildWhenPlaybackWasActive() {
        let episode = makeEpisode()
        var policy = PlaybackSessionPolicy()

        XCTAssertEqual(
            policy.handle(
                .mediaServicesWereReset(route: .builtIn),
                episodeID: episode.id,
                playbackRequested: true,
                didReachNaturalEnd: false
            ),
            .rebuildAndResume
        )
    }

    func testStaleEndCallbackCannotFinishReplacementItem() throws {
        let engine = AudioEngine()
        engine.load(makeEpisode())
        let staleItem = try XCTUnwrap(engine.player.currentItem)
        engine.load(makeEpisode())
        engine.setCurrentTime(41)

        engine.handleEndOfItem(staleItem)

        XCTAssertEqual(engine.currentTime, 41, accuracy: 0.001)
        XCTAssertFalse(engine.didReachNaturalEnd)
    }

    func testNotificationParserPreservesShouldResumeAsTypedEvent() {
        let notification = Notification(
            name: AVAudioSession.interruptionNotification,
            object: nil,
            userInfo: [
                AVAudioSessionInterruptionTypeKey: NSNumber(
                    value: AVAudioSession.InterruptionType.ended.rawValue
                ),
                AVAudioSessionInterruptionOptionKey: NSNumber(
                    value: AVAudioSession.InterruptionOptions.shouldResume.rawValue
                ),
            ]
        )

        XCTAssertEqual(
            PlaybackAudioSessionObserver.event(from: notification, currentRoute: .car),
            .interruptionEnded(shouldResume: true, route: .car)
        )
    }

    func testPlaybackFailureClassifiesUnderlyingOfflineError() {
        let wrapped = NSError(
            domain: AVFoundationErrorDomain,
            code: -11_800,
            userInfo: [NSUnderlyingErrorKey: URLError(.notConnectedToInternet)]
        )

        XCTAssertEqual(EngineError(wrapped).failure.code, .offline)
    }

    private func makeEpisode() -> Episode {
        let id = UUID()
        return Episode(
            id: id,
            podcastID: UUID(),
            guid: "qualification-\(id.uuidString)",
            title: "Qualification Episode",
            pubDate: Date(),
            duration: 600,
            enclosureURL: URL(string: "https://example.com/\(id.uuidString).mp3")!
        )
    }
}
