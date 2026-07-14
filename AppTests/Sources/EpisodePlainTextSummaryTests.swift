import XCTest
@testable import Podcastr

/// Coverage for `Episode.plainTextSummary` — the row-preview projection
/// used by every episode list (Library show detail, Search). The previous
/// implementation only stripped tags; HTML entities (`&rsquo;`, `&mdash;`,
/// `&hellip;`) bled through into the visible text. Now it routes through
/// `EpisodeShowNotesFormatter` for proper decoding.
final class EpisodePlainTextSummaryTests: XCTestCase {

    // MARK: - Helpers

    private func makeEpisode(description: String) -> Episode {
        Episode(
            podcastID: UUID(),
            guid: "test-\(UUID().uuidString)",
            title: "Test",
            description: description,
            pubDate: Date(),
            duration: nil,
            enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
    }

    // MARK: - Tags

    func testStripsSimpleHTMLTags() {
        let ep = makeEpisode(description: "<p>Hello <b>world</b>.</p>")
        XCTAssertEqual(ep.plainTextSummary, "Hello world.")
    }

    func testStripsAttributesInTags() {
        let ep = makeEpisode(description: "<a href=\"https://x.com\">link</a> text")
        XCTAssertEqual(ep.plainTextSummary, "link text")
    }

    // MARK: - Entity decoding (the regression we just fixed)

    func testDecodesCurlyApostrophe() {
        let ep = makeEpisode(description: "It&rsquo;s a beautiful day.")
        XCTAssertEqual(ep.plainTextSummary, "It\u{2019}s a beautiful day.")
    }

    func testDecodesEmDash() {
        let ep = makeEpisode(description: "Two ideas&mdash;one show.")
        XCTAssertEqual(ep.plainTextSummary, "Two ideas\u{2014}one show.")
    }

    func testDecodesAmpersand() {
        let ep = makeEpisode(description: "Black &amp; white photography")
        XCTAssertEqual(ep.plainTextSummary, "Black & white photography")
    }

    // MARK: - Numeric character references (WordPress + many feed gens)

    func testDecodesDecimalNumericApostrophe() {
        // `&#39;` is the most common one — WordPress emits it for every
        // straight apostrophe.
        let ep = makeEpisode(description: "It&#39;s a beautiful day.")
        XCTAssertEqual(ep.plainTextSummary, "It's a beautiful day.")
    }

    func testDecodesDecimalNumericRightSingleQuote() {
        let ep = makeEpisode(description: "It&#8217;s a beautiful day.")
        XCTAssertEqual(ep.plainTextSummary, "It\u{2019}s a beautiful day.")
    }

    func testDecodesHexNumericRightSingleQuote() {
        let ep = makeEpisode(description: "It&#x2019;s a beautiful day.")
        XCTAssertEqual(ep.plainTextSummary, "It\u{2019}s a beautiful day.")
    }

    func testDecodesHexUppercaseX() {
        let ep = makeEpisode(description: "It&#X2019;s a beautiful day.")
        XCTAssertEqual(ep.plainTextSummary, "It\u{2019}s a beautiful day.")
    }

    func testDecodesNumericEllipsis() {
        let ep = makeEpisode(description: "Hold on&#8230; here it comes.")
        XCTAssertEqual(ep.plainTextSummary, "Hold on\u{2026} here it comes.")
    }

    func testMixedNamedAndNumericEntities() {
        let ep = makeEpisode(description: "AT&amp;T&#8217;s &#x201C;Tech&#x201D; News")
        XCTAssertEqual(
            ep.plainTextSummary,
            "AT&T\u{2019}s \u{201C}Tech\u{201D} News"
        )
    }

    func testMalformedNumericEscapeStaysLiteral() {
        // A `&#` followed by non-digits isn't a valid ref — leave
        // it alone rather than silently dropping characters.
        let ep = makeEpisode(description: "look at &#abc; here")
        XCTAssertEqual(ep.plainTextSummary, "look at &#abc; here")
    }

    func testOutOfRangeNumericRefStaysLiteral() {
        // 0x110000 is past the last valid Unicode scalar — `Unicode.Scalar`
        // init returns nil and we leave the source verbatim.
        let ep = makeEpisode(description: "x&#1114112;y")
        XCTAssertEqual(ep.plainTextSummary, "x&#1114112;y")
    }

    // MARK: - Whitespace policy

    func testCollapsesParagraphBreaksToSingleLine() {
        // Multi-paragraph descriptions should fit on one line for row
        // previews. Both the formatter's `\n\n` separators and any extra
        // spacing collapse to a single space.
        let ep = makeEpisode(description: "<p>First paragraph.</p><p>Second paragraph.</p>")
        XCTAssertEqual(ep.plainTextSummary, "First paragraph. Second paragraph.")
    }

    func testTrimsLeadingAndTrailingWhitespace() {
        let ep = makeEpisode(description: "   leading and trailing   ")
        XCTAssertEqual(ep.plainTextSummary, "leading and trailing")
    }

    func testCollapsesInternalWhitespaceRuns() {
        let ep = makeEpisode(description: "Lots\t\tof    whitespace")
        XCTAssertEqual(ep.plainTextSummary, "Lots of whitespace")
    }

    // MARK: - Empty

    func testEmptyDescriptionReturnsEmpty() {
        let ep = makeEpisode(description: "")
        XCTAssertEqual(ep.plainTextSummary, "")
    }

    func testTagOnlyDescriptionReturnsEmpty() {
        let ep = makeEpisode(description: "<p></p>")
        XCTAssertEqual(ep.plainTextSummary, "")
    }
}
