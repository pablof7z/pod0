import XCTest
@testable import Podcastr

/// Coverage for `DateParsing.parseRFC822` — the cascade of RFC 822 / 1123 /
/// ISO 8601 shapes RSS publishers emit under `<pubDate>` in the wild.
///
/// Kept locale-stable assertions: `en_US_POSIX` formatter is used inside,
/// and we anchor the round-trip via `Date(timeIntervalSince1970:)` rather
/// than re-formatting back to a string.
@MainActor
final class RSSDateParsingTests: XCTestCase {

    // MARK: - Strict RFC 822 / 1123

    func testParsesStandardRFC1123WithNumericOffset() throws {
        // "Mon, 01 Jan 2024 12:00:00 +0000" → 2024-01-01T12:00:00Z
        let date = try XCTUnwrap(DateParsing.parseRFC822("Mon, 01 Jan 2024 12:00:00 +0000"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400)
    }

    func testParsesStandardRFC822WithTimezoneAbbreviation() throws {
        // GMT == +0000, same instant as the numeric-offset case above.
        let date = try XCTUnwrap(DateParsing.parseRFC822("Mon, 01 Jan 2024 12:00:00 GMT"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400)
    }

    // MARK: - Tolerant variants

    func testParsesSingleDigitDay() throws {
        // Some publishers emit "Mon, 1 Jan 2024 …" instead of "01 Jan".
        let date = try XCTUnwrap(DateParsing.parseRFC822("Mon, 1 Jan 2024 12:00:00 +0000"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400)
    }

    func testParsesWithoutSeconds() throws {
        // "EEE, dd MMM yyyy HH:mm zzz" — seconds omitted.
        let date = try XCTUnwrap(DateParsing.parseRFC822("Mon, 01 Jan 2024 12:00 GMT"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400)
    }

    func testParsesWithoutWeekdayPrefix() throws {
        // "dd MMM yyyy HH:mm:ss zzz" — no weekday.
        let date = try XCTUnwrap(DateParsing.parseRFC822("01 Jan 2024 12:00:00 GMT"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400)
    }

    // MARK: - Non-UTC offsets

    func testRespectsPositiveOffset() throws {
        // 12:00 +0500 is 07:00 UTC.
        let date = try XCTUnwrap(DateParsing.parseRFC822("Mon, 01 Jan 2024 12:00:00 +0500"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400 - 5 * 3600)
    }

    func testRespectsNegativeOffset() throws {
        // 12:00 -0800 is 20:00 UTC.
        let date = try XCTUnwrap(DateParsing.parseRFC822("Mon, 01 Jan 2024 12:00:00 -0800"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400 + 8 * 3600)
    }

    // MARK: - ISO 8601 fallback

    func testFallsBackToISO8601() throws {
        // Some Atom-flavored feeds emit ISO 8601 under <pubDate>.
        let date = try XCTUnwrap(DateParsing.parseRFC822("2024-01-01T12:00:00Z"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400)
    }

    func testFallsBackToISO8601WithFractionalSeconds() throws {
        let date = try XCTUnwrap(DateParsing.parseRFC822("2024-01-01T12:00:00.500Z"))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400.5, accuracy: 0.01)
    }

    // MARK: - Failure surface

    func testReturnsNilForEmptyString() {
        XCTAssertNil(DateParsing.parseRFC822(""))
    }

    func testReturnsNilForWhitespaceOnly() {
        XCTAssertNil(DateParsing.parseRFC822("   \n\t   "))
    }

    func testReturnsNilForGarbage() {
        XCTAssertNil(DateParsing.parseRFC822("not a date"))
    }

    func testReturnsNilForDateOnly() {
        // "01 Jan 2024" has no time component — none of our formats accept it.
        XCTAssertNil(DateParsing.parseRFC822("01 Jan 2024"))
    }

    // MARK: - Whitespace tolerance

    func testTrimsLeadingAndTrailingWhitespace() throws {
        let date = try XCTUnwrap(DateParsing.parseRFC822("  Mon, 01 Jan 2024 12:00:00 GMT  "))
        XCTAssertEqual(date.timeIntervalSince1970, 1_704_110_400)
    }
}
