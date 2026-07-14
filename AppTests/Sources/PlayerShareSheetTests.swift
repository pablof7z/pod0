import XCTest
@testable import Podcastr

/// Predicate-level coverage for `PlayerShareSheet`. We can't render the SwiftUI
/// hierarchy from XCTest without an `@MainActor` host, so we keep these tests
/// pinned to the pure helpers — the same model the view consumes — and let the
/// snapshot/UI lane catch any visual regressions.
@MainActor
final class PlayerShareSheetTests: XCTestCase {

    // MARK: - Timestamped-share gate

    /// Boundary: the gate is *strictly* greater than 5s, so a fresh start at
    /// exactly 5s does not surface the "Copy link at current time" row.
    /// Anything below should be hidden too.
    func testIsMeaningfulPlayheadHidesRowForFreshStart() {
        XCTAssertFalse(PlayerShareSheet.isMeaningfulPlayhead(0))
        XCTAssertFalse(PlayerShareSheet.isMeaningfulPlayhead(2.4))
        XCTAssertFalse(PlayerShareSheet.isMeaningfulPlayhead(5))
    }

    /// Once the user is past the lead-in we expose the timestamped row.
    /// Includes a deliberately chunky value to confirm the predicate isn't
    /// accidentally wrapping a Bool the wrong way.
    func testIsMeaningfulPlayheadShowsRowOncePastLeadIn() {
        XCTAssertTrue(PlayerShareSheet.isMeaningfulPlayhead(5.01))
        XCTAssertTrue(PlayerShareSheet.isMeaningfulPlayhead(30))
        XCTAssertTrue(PlayerShareSheet.isMeaningfulPlayhead(3_600))
    }

    /// Defensive: an engine that hasn't yet reported a position will surface
    /// either zero or, in pathological cases, a negative scrub-preview value.
    /// Either way we must hide the row — we never want to emit `?t=-3` URLs.
    func testIsMeaningfulPlayheadHidesRowForNegativeOrZeroPlayhead() {
        XCTAssertFalse(PlayerShareSheet.isMeaningfulPlayhead(-1))
        XCTAssertFalse(PlayerShareSheet.isMeaningfulPlayhead(-100))
        XCTAssertFalse(PlayerShareSheet.isMeaningfulPlayhead(0))
    }
}
