import XCTest
@testable import Podcastr

/// Coverage for `PlaybackState.sleepTimerChipLabel` — the live countdown
/// the player's action cluster reads when the sleep timer is armed.
///
/// The chip used to print the static preset string ("30 min") for the
/// entire armed window. The new label reaches into
/// `engine.sleepTimer.phase` so SwiftUI's @Observable tracking refreshes
/// the chip on every tick.
@MainActor
final class PlaybackSleepTimerLabelTests: XCTestCase {

    func testIdleReadsSleep() {
        let state = PlaybackState()
        XCTAssertEqual(state.sleepTimerChipLabel, "Sleep")
    }

    func testArmedDurationFormatsAsClock() {
        let state = PlaybackState()
        // 30 min preset → engine arms with 1800s remaining.
        state.setSleepTimer(.minutes(30))
        XCTAssertEqual(state.sleepTimerChipLabel, "30:00")
    }

    func testArmedSubMinuteShowsSeconds() {
        let state = PlaybackState()
        // 0.5s preset isn't a real preset but the duration is what matters
        // — drive the engine directly to test the format edge.
        state.engine.sleepTimer.set(.duration(45))
        XCTAssertEqual(state.sleepTimerChipLabel, "0:45")
    }

    func testArmedOverHourShowsHours() {
        let state = PlaybackState()
        state.engine.sleepTimer.set(.duration(60 * 75))  // 1h 15m
        XCTAssertEqual(state.sleepTimerChipLabel, "1:15:00")
    }

    func testEndOfEpisodeReadsEnd() {
        let state = PlaybackState()
        state.setSleepTimer(.endOfEpisode)
        XCTAssertEqual(state.sleepTimerChipLabel, "End")
    }

    func testCancellingReturnsToSleep() {
        let state = PlaybackState()
        state.setSleepTimer(.minutes(15))
        XCTAssertNotEqual(state.sleepTimerChipLabel, "Sleep")
        state.setSleepTimer(.off)
        XCTAssertEqual(state.sleepTimerChipLabel, "Sleep")
    }
}
