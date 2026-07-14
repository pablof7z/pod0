import MediaPlayer
import UIKit
import XCTest
@testable import Podcastr

@MainActor
final class PlaybackStateAudioCallbackTests: XCTestCase {

    func testRemoteSeekUsesPlaybackStatePersistenceSideEffects() {
        let state = PlaybackState()
        let episode = makeEpisode(duration: 600)
        var persisted: [(UUID, TimeInterval)] = []
        var didFlush = false
        state.onPersistPosition = { id, position in
            persisted.append((id, position))
        }
        state.onFlushPositions = {
            didFlush = true
        }
        state.setEpisode(episode)

        let status = state.engine.nowPlaying.performRemoteCommand(.seek(123))

        XCTAssertEqual(status, .success)
        XCTAssertEqual(state.engine.currentTime, 123, accuracy: 0.001)
        XCTAssertEqual(persisted.count, 1)
        XCTAssertEqual(persisted.first?.0, episode.id)
        XCTAssertEqual(persisted.first?.1 ?? 0, 123, accuracy: 0.001)
        XCTAssertTrue(didFlush)
    }

    func testRemotePauseUsesPlaybackStateFlushSideEffect() {
        let state = PlaybackState()
        state.setEpisode(makeEpisode())
        var didFlush = false
        state.onFlushPositions = {
            didFlush = true
        }

        let status = state.engine.nowPlaying.performRemoteCommand(.pause)

        XCTAssertEqual(status, .success)
        XCTAssertTrue(didFlush)
    }

    func testSleepTimerFireUsesPlaybackStatePauseSideEffects() {
        let state = PlaybackState()
        state.setEpisode(makeEpisode())
        var didFlush = false
        state.onFlushPositions = {
            didFlush = true
        }

        state.engine.sleepTimer.onFire()

        XCTAssertTrue(didFlush)
    }

    func testSameEpisodeRefreshUpdatesEngineEpisodeMetadata() {
        let state = PlaybackState()
        let id = UUID()
        let original = makeEpisode(id: id, title: "Original", duration: 600)
        let refreshed = makeEpisode(id: id, title: "Refreshed", duration: 720)

        state.setEpisode(original)
        state.setEpisode(refreshed)

        XCTAssertEqual(state.episode?.title, "Refreshed")
        XCTAssertEqual(state.engine.episode?.title, "Refreshed")
        XCTAssertEqual(state.engine.duration, 720, accuracy: 0.001)
    }

    func testSameEpisodeReplayAfterNaturalEndSeeksToStart() {
        let state = PlaybackState()
        let episode = makeEpisode(duration: 60)
        state.setEpisode(episode)
        state.engine.seek(to: 60)
        state.engine.didReachNaturalEnd = true

        state.setEpisode(episode)

        XCTAssertEqual(state.engine.currentTime, 0, accuracy: 0.001)
        XCTAssertFalse(state.engine.didReachNaturalEnd)
    }

    func testNowPlayingDoesNotPublishPreviousArtworkWhenNextEpisodeHasNone() {
        let engine = AudioEngine()
        let oldArtworkURL = URL(string: "https://example.com/old.png")!
        engine.lastPublishedArtworkURL = oldArtworkURL
        engine.lastPublishedArtworkImage = makeImage()
        engine.resolveArtworkURL = { _, _ in nil }
        defer { engine.nowPlaying.clear() }

        engine.load(makeEpisode(title: "Episode without artwork"))

        let info = MPNowPlayingInfoCenter.default().nowPlayingInfo
        XCTAssertNil(info?[MPMediaItemPropertyArtwork])
        XCTAssertNil(engine.lastPublishedArtworkImage)
    }

    private func makeEpisode(
        id: UUID = UUID(),
        title: String = "Episode",
        duration: TimeInterval = 300
    ) -> Episode {
        Episode(
            id: id,
            podcastID: UUID(),
            guid: "episode-\(id.uuidString)",
            title: title,
            pubDate: Date(),
            duration: duration,
            enclosureURL: URL(string: "https://example.com/\(id.uuidString).mp3")!
        )
    }

    private func makeImage() -> UIImage {
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8))
        return renderer.image { context in
            UIColor.red.setFill()
            context.fill(CGRect(origin: .zero, size: CGSize(width: 8, height: 8)))
        }
    }
}
