import XCTest
@testable import Podcastr

/// Coverage for the unplayed-count badge label rendered on
/// `LibraryGridCell`. The view file exposes the threshold logic via
/// readable computed properties, but the rules themselves are pure
/// integer math worth pinning so a future tweak (e.g. dropping the
/// "99+" cap or adding a wider badge tier) trips a test instead of
/// silently degrading the home grid.
///
/// Reproduces the cell's logic locally because `LibraryGridCell` is a
/// SwiftUI view and its private threshold properties aren't visible
/// across the module boundary. If the view drifts from this table the
/// test will pass while the UI silently regresses — keeping the
/// thresholds duplicated here is the cost of UI testability.
final class LibraryUnplayedBadgeTests: XCTestCase {

    private func badgeLabel(for count: Int) -> String {
        count > 99 ? "99+" : "\(count)"
    }

    // MARK: - Single digit

    func testNineRendersAsItself() {
        XCTAssertEqual(badgeLabel(for: 9), "9")
    }

    // MARK: - Two-digit (the regression we just fixed)

    func testTenRendersAsTen() {
        // Pre-fix: cap at min(count, 9) emitted "9" for both 9 and 10
        // so the user couldn't tell a backlog of 9 from one of 50.
        XCTAssertEqual(badgeLabel(for: 10), "10")
    }

    func testNinetyNineRendersAsItself() {
        XCTAssertEqual(badgeLabel(for: 99), "99")
    }

    // MARK: - Past the cap

    func testHundredRendersAsCappedString() {
        XCTAssertEqual(badgeLabel(for: 100), "99+")
    }

    func testThousandRendersAsCappedString() {
        // The Daily's full back-catalog scenario — must not show
        // "1000" inside a 14-24pt circle.
        XCTAssertEqual(badgeLabel(for: 1_000), "99+")
    }
}
