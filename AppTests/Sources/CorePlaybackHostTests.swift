import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CorePlaybackHostTests: XCTestCase {
    func testRealAudioEngineExecutesTypedLoadSeekRatePauseAndTimerRequests() throws {
        let episode = makeEpisode()
        let engine = AudioEngine()
        let host = CorePlaybackHost(engine: engine) { id in
            id == episode.id ? episode : nil
        }
        let episodeID = EpisodeId(uuid: episode.id)

        let loaded = host.execute(.loadMedia(
            episodeId: episodeID,
            audioUrl: episode.enclosureURL.absoluteString,
            startPositionMilliseconds: 12_500
        ))
        guard case .playbackObserved(let loadObservation) = loaded else {
            return XCTFail("Expected a typed load observation")
        }
        XCTAssertEqual(loadObservation.episodeId, episodeID)
        XCTAssertEqual(loadObservation.state, .loading)
        XCTAssertEqual(loadObservation.positionMilliseconds, 12_500)

        let played = host.execute(.play(
            episodeId: episodeID,
            transitionCue: .immediate
        ))
        guard case .playbackObserved(let playObservation) = played else {
            return XCTFail("Expected a typed play observation")
        }
        XCTAssertEqual(playObservation.state, .playing)

        _ = host.execute(.seek(episodeId: episodeID, positionMilliseconds: 21_250))
        _ = host.execute(.setRate(
            episodeId: episodeID,
            rate: PlaybackRatePermille(value: 1_750)
        ))
        _ = host.execute(.armNativeTimer(
            episodeId: episodeID,
            mode: .duration(durationMilliseconds: 60_000)
        ))
        _ = host.execute(.pause(episodeId: episodeID))

        XCTAssertEqual(engine.currentTime, 21.25, accuracy: 0.001)
        XCTAssertEqual(engine.rate, 1.75, accuracy: 0.001)
        XCTAssertEqual(engine.sleepTimer.mode, .duration(60))
        XCTAssertEqual(engine.state, .paused)
        _ = host.execute(.cancelNativeTimer(episodeId: episodeID))
        XCTAssertEqual(engine.sleepTimer.mode, .off)
    }

    func testPreparedInterruptionAndRouteChangesUseGeneratedLifecycleContract() {
        let episode = makeEpisode()
        let engine = AudioEngine()
        let host = CorePlaybackHost(engine: engine) { _ in episode }
        let episodeID = EpisodeId(uuid: episode.id)
        var observations: [PlaybackLifecycleObservation] = []
        host.installObservationSink { observations.append($0) }
        _ = host.execute(.loadMedia(
            episodeId: episodeID,
            audioUrl: episode.enclosureURL.absoluteString,
            startPositionMilliseconds: 0
        ))

        engine.setState(.paused)
        engine.onHostAudioSessionEvent(.interruptionBegan(route: .bluetooth))

        XCTAssertTrue(observations.contains { $0.state == .prepared })
        XCTAssertEqual(observations.last?.episodeId, episodeID)
        XCTAssertEqual(observations.last?.route, .bluetooth)
        XCTAssertEqual(observations.last?.interruption, .began)
    }

    func testStaleEpisodeAndUnsupportedTimerFailWithoutExecutingMediaEffects() {
        let episode = makeEpisode()
        let engine = AudioEngine()
        let host = CorePlaybackHost(engine: engine) { _ in episode }
        let loadedID = EpisodeId(uuid: episode.id)
        _ = host.execute(.loadMedia(
            episodeId: loadedID,
            audioUrl: episode.enclosureURL.absoluteString,
            startPositionMilliseconds: 0
        ))

        let stale = host.execute(.seek(
            episodeId: EpisodeId(uuid: UUID()),
            positionMilliseconds: 99_000
        ))
        guard case .failed(code: .mediaUnavailable, safeDetail: _) = stale else {
            return XCTFail("Expected stale episode rejection")
        }
        XCTAssertEqual(engine.currentTime, 0)

        let unsupported = host.execute(.armNativeTimer(
            episodeId: loadedID,
            mode: .unsupported(wireCode: 900)
        ))
        guard case .failed(code: .invalidResponse, safeDetail: _) = unsupported else {
            return XCTFail("Expected unsupported timer rejection")
        }
    }

    private func makeEpisode() -> Episode {
        let id = UUID()
        return Episode(
            id: id,
            podcastID: UUID(),
            guid: "host-\(id.uuidString)",
            title: "Native Host Episode",
            pubDate: Date(),
            duration: 600,
            enclosureURL: URL(string: "https://cdn.example.test/\(id.uuidString).mp3")!
        )
    }
}
