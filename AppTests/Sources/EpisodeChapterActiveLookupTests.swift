import XCTest
@testable import Podcastr

/// Coverage for the `Collection<Episode.Chapter>.active(at:)` helper that
/// drives active-chapter highlighting on the player and follow-along
/// surfaces. Both `PlayerChaptersScrollView` and
/// `EpisodeDetailView.activeChapterID(in:)` route through this helper, so a
/// regression here is a regression on every chapter-aware screen.
final class EpisodeChapterActiveLookupTests: XCTestCase {

    /// Three back-to-back chapters with a gap between 2 and 3 so we can
    /// probe both the "exact boundary" and "between chapters" cases.
    ///
    ///   ch1: 0...
    ///   ch2: 252...
    ///   ch3: 1720...   (1468s gap from ch2 end)
    private let chapters: [Episode.Chapter] = [
        .init(startTime: 0, title: "Cold open"),
        .init(startTime: 252, title: "Why ketones matter"),
        .init(startTime: 1720, title: "The Inuit objection"),
    ]

    func testReturnsFirstChapterAtTimeZero() {
        XCTAssertEqual(chapters.active(at: 0)?.title, "Cold open")
    }

    func testReturnsCurrentChapterMidway() {
        XCTAssertEqual(chapters.active(at: 500)?.title, "Why ketones matter")
    }

    func testReturnsExactBoundaryChapter() {
        // At t=252 we should be in chapter 2, not still in chapter 1.
        XCTAssertEqual(chapters.active(at: 252)?.title, "Why ketones matter")
    }

    func testReturnsLastChapterPastFinalStart() {
        // Far past the last chapter's start, the last chapter remains active
        // until the next one — there isn't one, so it stays active.
        XCTAssertEqual(chapters.active(at: 9_999)?.title, "The Inuit objection")
    }

    func testReturnsFirstChapterWhenPlayheadIsBeforeAnyChapter() {
        // Defensive: chapters whose first start > 0 (e.g. an intro that
        // starts at 5s). At t=0 we still want an active indicator, not nil.
        let lateStart: [Episode.Chapter] = [
            .init(startTime: 5, title: "Intro"),
            .init(startTime: 100, title: "Topic"),
        ]
        XCTAssertEqual(lateStart.active(at: 0)?.title, "Intro")
        XCTAssertEqual(lateStart.active(at: 4.999)?.title, "Intro")
    }

    func testReturnsNilForEmptyCollection() {
        XCTAssertNil([Episode.Chapter]().active(at: 100))
    }

    func testHandlesNegativePlayhead() {
        // Defensive: the engine briefly reports negative `currentTime`
        // during scrub-to-zero. We treat that as "before the timeline" and
        // fall back to the first chapter.
        XCTAssertEqual(chapters.active(at: -1)?.title, "Cold open")
    }
}
