import XCTest
import SwiftUI
@testable import Podcastr

/// Coverage for `HighlightedText.makeAttributed(text:query:)`. The
/// previous implementation lowercased both inputs and walked grapheme
/// offsets in the lowercased copy back into the original — which
/// silently drifted for inputs where lowercasing changed grapheme
/// counts and missed Unicode-folded matches. These tests lock the
/// fix in.
final class HighlightedTextTests: XCTestCase {

    // MARK: - Helpers

    /// Number of contiguous runs whose `.font` matches the highlight
    /// font. Useful as a "how many highlights are there" assertion
    /// without coupling to character offsets that depend on the
    /// specific accent attribute we set.
    private func highlightRunCount(_ s: AttributedString) -> Int {
        var count = 0
        for run in s.runs where run.font == Font.body.bold() {
            count += 1
        }
        return count
    }

    /// Concatenation of every highlighted slice in document order.
    /// Lets us assert *which* characters were highlighted, not just
    /// how many regions.
    private func highlightedSlices(_ s: AttributedString) -> String {
        var out = ""
        for run in s.runs where run.font == Font.body.bold() {
            out += String(s[run.range].characters)
        }
        return out
    }

    // MARK: - Empty / no-match cases

    func testEmptyQueryProducesNoHighlights() {
        let attr = HighlightedText.makeAttributed(text: "Hello world", query: "")
        XCTAssertEqual(highlightRunCount(attr), 0)
    }

    func testWhitespaceOnlyQueryProducesNoHighlights() {
        let attr = HighlightedText.makeAttributed(text: "Hello world", query: "   ")
        XCTAssertEqual(highlightRunCount(attr), 0)
    }

    func testNoMatchProducesNoHighlights() {
        let attr = HighlightedText.makeAttributed(text: "Hello world", query: "zzz")
        XCTAssertEqual(highlightRunCount(attr), 0)
    }

    // MARK: - Simple matches

    func testSingleAsciiMatch() {
        let attr = HighlightedText.makeAttributed(text: "Hello world", query: "world")
        XCTAssertEqual(highlightRunCount(attr), 1)
        XCTAssertEqual(highlightedSlices(attr), "world")
    }

    func testCaseInsensitiveAsciiMatch() {
        let attr = HighlightedText.makeAttributed(text: "Hello WORLD", query: "world")
        XCTAssertEqual(highlightRunCount(attr), 1)
        XCTAssertEqual(highlightedSlices(attr), "WORLD")
    }

    func testMultipleMatches() {
        let attr = HighlightedText.makeAttributed(text: "the quick brown fox the lazy dog", query: "the")
        XCTAssertEqual(highlightRunCount(attr), 2)
        XCTAssertEqual(highlightedSlices(attr), "thethe")
    }

    func testQueryIsTrimmedBeforeSearch() {
        // Leading/trailing whitespace on the query should be stripped
        // (the search bar passes raw input). This mirrors how the
        // surface uses `.trimmed`.
        let attr = HighlightedText.makeAttributed(text: "Hello world", query: "  world  ")
        XCTAssertEqual(highlightRunCount(attr), 1)
        XCTAssertEqual(highlightedSlices(attr), "world")
    }

    // MARK: - Unicode correctness

    func testGraphemeMutatingLowercaseDoesNotDriftHighlight() {
        // Turkish capital "İ" (U+0130) lowercases to "i\u{0307}" — the
        // grapheme-cluster count of the lowercased string differs from
        // the original. The legacy implementation used those drifted
        // offsets to index back into the original and would highlight
        // the wrong characters; the new implementation searches the
        // original text directly, so the offsets are stable.
        let attr = HighlightedText.makeAttributed(text: "İstanbul", query: "stan")
        XCTAssertEqual(highlightRunCount(attr), 1)
        XCTAssertEqual(highlightedSlices(attr), "stan")
    }

    func testCaseInsensitiveOptionAppliesUnicodeFold() {
        // German "ß" matches "SS" / "ss" / "Ss" / "sS" under
        // Foundation's `.caseInsensitive` fold. The previous
        // implementation literal-matched lowercased copies, which
        // produced "ß" vs "ss" — an inequality, so this case used to
        // miss entirely. The new implementation finds it.
        let attr = HighlightedText.makeAttributed(text: "Straße", query: "STRASSE")
        XCTAssertEqual(highlightRunCount(attr), 1)
        // The match is on the original "Straße", not the query, so we
        // see the original characters in the highlight.
        XCTAssertEqual(highlightedSlices(attr), "Straße")
    }
}
