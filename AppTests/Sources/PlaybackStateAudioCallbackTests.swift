import MediaPlayer
import UIKit
import XCTest
@testable import Podcastr

/// Native now-playing presentation checks. Remote command routing is exercised
/// by `SharedPlaybackVerticalSliceTests`; Rust owns its durable side effects.
@MainActor
final class PlaybackStateAudioCallbackTests: XCTestCase {
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

    private func makeEpisode(title: String) -> Episode {
        let id = UUID()
        return Episode(
            id: id,
            podcastID: UUID(),
            guid: "episode-\(id.uuidString)",
            title: title,
            pubDate: Date(),
            duration: 300,
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
