import XCTest
@testable import Podcastr

/// Boundary coverage for `RelativeTimestamp.compact` / `.extended`.
///
/// The previous implementation skipped the weeks bucket — content
/// 8 days old jumped from "7d ago" straight to a calendar date. It
/// also rendered future dates as negative numbers like "-3s ago"
/// when the timestamp came in slightly ahead of the local clock
/// (clock skew on imported content). Both bugs are locked here.
final class RelativeTimestampTests: XCTestCase {

    /// Pinned `now` so every test runs against a stable reference
    /// point — `Date()` would race with the assertion otherwise.
    private let now = Date(timeIntervalSince1970: 1_700_000_000)

    private func ago(_ seconds: TimeInterval) -> Date {
        now.addingTimeInterval(-seconds)
    }

    // MARK: - compact

    func testCompactJustNowBelowThreshold() {
        XCTAssertEqual(RelativeTimestamp.compact(ago(0), now: now), "just now")
        XCTAssertEqual(RelativeTimestamp.compact(ago(4), now: now), "just now")
    }

    func testCompactSeconds() {
        XCTAssertEqual(RelativeTimestamp.compact(ago(5), now: now), "5s ago")
        XCTAssertEqual(RelativeTimestamp.compact(ago(59), now: now), "59s ago")
    }

    func testCompactMinutes() {
        XCTAssertEqual(RelativeTimestamp.compact(ago(60), now: now), "1m ago")
        XCTAssertEqual(RelativeTimestamp.compact(ago(3_599), now: now), "59m ago")
    }

    func testCompactHours() {
        XCTAssertEqual(RelativeTimestamp.compact(ago(3_600), now: now), "1h ago")
        XCTAssertEqual(RelativeTimestamp.compact(ago(86_400), now: now), "24h ago")
    }

    func testCompactFutureClampsToJustNow() {
        // Negative interval — timestamp is in the future. Used to
        // render "-3s ago"; should now collapse to the just-now bucket.
        XCTAssertEqual(RelativeTimestamp.compact(now.addingTimeInterval(3), now: now), "just now")
        XCTAssertEqual(RelativeTimestamp.compact(now.addingTimeInterval(3600), now: now), "just now")
    }

    // MARK: - extended

    func testExtendedJustNowBelowOneMinute() {
        XCTAssertEqual(RelativeTimestamp.extended(ago(0), now: now), "just now")
        XCTAssertEqual(RelativeTimestamp.extended(ago(59), now: now), "just now")
    }

    func testExtendedMinutes() {
        XCTAssertEqual(RelativeTimestamp.extended(ago(60), now: now), "1m ago")
        XCTAssertEqual(RelativeTimestamp.extended(ago(3_599), now: now), "59m ago")
    }

    func testExtendedHours() {
        XCTAssertEqual(RelativeTimestamp.extended(ago(3_600), now: now), "1h ago")
        XCTAssertEqual(RelativeTimestamp.extended(ago(86_399), now: now), "23h ago")
    }

    func testExtendedDays() {
        XCTAssertEqual(RelativeTimestamp.extended(ago(86_400), now: now), "1d ago")
        XCTAssertEqual(RelativeTimestamp.extended(ago(7 * 86_400 - 1), now: now), "6d ago")
    }

    func testExtendedWeeks() {
        // The bug fix — was skipping straight to a calendar date here.
        XCTAssertEqual(RelativeTimestamp.extended(ago(7 * 86_400), now: now), "1w ago")
        XCTAssertEqual(RelativeTimestamp.extended(ago(8 * 86_400), now: now), "1w ago")
        XCTAssertEqual(RelativeTimestamp.extended(ago(21 * 86_400), now: now), "3w ago")
    }

    func testExtendedFallsBackToDateAtFourWeeks() {
        // 4 weeks is the cutover — anything older shows the absolute date.
        // Don't hard-code the formatted string (it varies by locale and
        // current iOS version), but assert that *something* non-relative
        // is returned (no "ago" suffix).
        let formatted = RelativeTimestamp.extended(ago(4 * 7 * 86_400), now: now)
        XCTAssertFalse(formatted.contains("ago"))
        XCTAssertFalse(formatted.contains("just now"))
    }

    func testExtendedFutureClampsToJustNow() {
        XCTAssertEqual(RelativeTimestamp.extended(now.addingTimeInterval(3), now: now), "just now")
        XCTAssertEqual(RelativeTimestamp.extended(now.addingTimeInterval(86_400), now: now), "just now")
    }
}
