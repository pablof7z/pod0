import AVFoundation
import XCTest
@testable import Podcastr

/// Native lifecycle parsing and AVFoundation safety checks. Durable
/// interruption, route-loss, resume, and completion policy is covered by the
/// Rust facade tests and the typed `CorePlaybackHost` integration tests.
@MainActor
final class PlaybackLifecycleQualificationTests: XCTestCase {
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
