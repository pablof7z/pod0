import XCTest
@testable import Podcastr

/// Coverage for `PlaybackState`'s pure chapter-nav helpers
/// (`nextChapter(after:in:)` and `previousChapter(from:in:restartThreshold:)`)
/// — the long-press skip-button shortcuts wired in `PlayerControlsView`.
///
/// The methods live on `PlaybackState` because they're conceptually a
/// playback action, but the resolution logic is pure on chapter+playhead so
/// we can pin behaviour without spinning up the audio engine.
final class PlaybackChapterNavigationTests: XCTestCase {

    /// Three back-to-back chapters with explicit start times.
    ///   ch1: 0
    ///   ch2: 252
    ///   ch3: 1720
    private let chapters: [Episode.Chapter] = [
        .init(startTime: 0, title: "Cold open"),
        .init(startTime: 252, title: "Why ketones matter"),
        .init(startTime: 1720, title: "The Inuit objection"),
    ]

    // MARK: - Next chapter

    func testNextChapterFromMidwayThroughCurrent() {
        let next = PlaybackState.nextChapter(after: 500, in: chapters)
        XCTAssertEqual(next?.title, "The Inuit objection")
    }

    func testNextChapterFromBeforeFirstReturnsSecond() {
        // We're inside chapter 1 (or before any chapter). "Next" goes to ch2.
        let next = PlaybackState.nextChapter(after: 0, in: chapters)
        XCTAssertEqual(next?.title, "Why ketones matter")
    }

    func testNextChapterAtExactStartOfCurrentReturnsFollowing() {
        // Boundary: at 252 we're at the start of ch2, so "next" should be ch3.
        let next = PlaybackState.nextChapter(after: 252, in: chapters)
        XCTAssertEqual(next?.title, "The Inuit objection")
    }

    func testNextChapterReturnsNilWhenAtLast() {
        XCTAssertNil(PlaybackState.nextChapter(after: 9_999, in: chapters))
    }

    func testNextChapterReturnsNilForEmpty() {
        XCTAssertNil(PlaybackState.nextChapter(after: 100, in: []))
    }

    // MARK: - Previous chapter (iOS Music pattern)

    func testPreviousChapterRestartsCurrentWhenPastThreshold() {
        // We're 100s into ch2 — past the 3s threshold — so "previous" restarts ch2.
        let prev = PlaybackState.previousChapter(
            from: 352,
            in: chapters,
            restartThreshold: 3.0
        )
        XCTAssertEqual(prev?.title, "Why ketones matter")
        XCTAssertEqual(prev?.startTime, 252)
    }

    func testPreviousChapterStepsBackWhenWithinThreshold() {
        // We're 1s into ch2 — within the 3s threshold — so "previous" goes to ch1.
        let prev = PlaybackState.previousChapter(
            from: 253,
            in: chapters,
            restartThreshold: 3.0
        )
        XCTAssertEqual(prev?.title, "Cold open")
    }

    func testPreviousChapterAtExactStartGoesToPrior() {
        // At t=252 we're at the exact start of ch2 (elapsed = 0). Within
        // threshold → step back to ch1.
        let prev = PlaybackState.previousChapter(
            from: 252,
            in: chapters,
            restartThreshold: 3.0
        )
        XCTAssertEqual(prev?.title, "Cold open")
    }

    func testPreviousChapterFromFirstReturnsFirst() {
        // We're 1s into ch1, no chapter before — clamp to ch1 (no-op restart).
        let prev = PlaybackState.previousChapter(
            from: 1,
            in: chapters,
            restartThreshold: 3.0
        )
        XCTAssertEqual(prev?.title, "Cold open")
    }

    func testPreviousChapterReturnsNilForEmpty() {
        XCTAssertNil(PlaybackState.previousChapter(
            from: 100,
            in: [],
            restartThreshold: 3.0
        ))
    }

    func testPreviousChapterRestartThresholdIsConfigurable() {
        // With threshold=10, being 5s into ch2 should still step back to ch1.
        let prev = PlaybackState.previousChapter(
            from: 257,
            in: chapters,
            restartThreshold: 10.0
        )
        XCTAssertEqual(prev?.title, "Cold open")
    }
}
