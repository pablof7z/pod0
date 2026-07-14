import XCTest
@testable import Podcastr

/// Coverage for `ChaptersClient.decode(_:)` — the pure JSON decode path.
/// The `fetch(url:)` path itself is just a URLSession round-trip; the
/// interesting behaviour lives in the decoder's permissive handling of
/// real-world Podcasting 2.0 chapter feeds.
final class ChaptersClientDecodeTests: XCTestCase {

    func testDecodesCanonicalSpec() throws {
        let json = """
        {
          "version": "1.2.0",
          "chapters": [
            {"startTime": 0, "title": "Cold open"},
            {"startTime": 252, "title": "Why ketones matter", "img": "https://example.com/c2.jpg"},
            {"startTime": 1720, "title": "The Inuit objection", "url": "https://example.com/notes#inuit"},
            {"startTime": 4810, "title": "Practical protocols", "endTime": 5400}
          ]
        }
        """.data(using: .utf8)!

        let chapters = try ChaptersClient.decode(json)

        XCTAssertEqual(chapters.count, 4)
        XCTAssertEqual(chapters[0].title, "Cold open")
        XCTAssertEqual(chapters[1].imageURL?.absoluteString, "https://example.com/c2.jpg")
        XCTAssertEqual(chapters[2].linkURL?.absoluteString, "https://example.com/notes#inuit")
        XCTAssertEqual(chapters[3].endTime, 5400)
        XCTAssertTrue(chapters.allSatisfy(\.includeInTableOfContents))
    }

    func testSortsByStartTimeWhenInputIsUnordered() throws {
        // Some publishers emit chapters in random order — the decoder
        // sorts so the active-chapter helper's `last(where:)` works.
        let json = """
        {"chapters": [
          {"startTime": 1000, "title": "Late"},
          {"startTime": 0, "title": "Early"},
          {"startTime": 250, "title": "Middle"}
        ]}
        """.data(using: .utf8)!
        let chapters = try ChaptersClient.decode(json)
        XCTAssertEqual(chapters.map(\.title), ["Early", "Middle", "Late"])
    }

    func testHonorsTocFalseAsNonNavigable() throws {
        // `toc: false` flags ad markers / non-navigable chapters per the
        // Podcasting 2.0 spec. The decoder preserves the flag; downstream
        // consumers (`navigableChapters`) filter it out.
        let json = """
        {"chapters": [
          {"startTime": 0, "title": "Show"},
          {"startTime": 600, "title": "Mid-roll ad", "toc": false},
          {"startTime": 720, "title": "Resume"}
        ]}
        """.data(using: .utf8)!
        let chapters = try ChaptersClient.decode(json)
        XCTAssertEqual(chapters.count, 3)
        XCTAssertEqual(chapters[1].includeInTableOfContents, false)
        XCTAssertEqual(chapters[0].includeInTableOfContents, true)
    }

    func testSkipsTitlelessEntries() throws {
        // Real-world feeds occasionally publish title-less ad markers.
        // The spec requires `title`; we drop these entirely rather than
        // surfacing blank rows to the user.
        let json = """
        {"chapters": [
          {"startTime": 0, "title": "Cold open"},
          {"startTime": 100},
          {"startTime": 200, "title": "   "},
          {"startTime": 300, "title": "Topic"}
        ]}
        """.data(using: .utf8)!
        let chapters = try ChaptersClient.decode(json)
        XCTAssertEqual(chapters.map(\.title), ["Cold open", "Topic"])
    }

    func testAcceptsIntegerAndFloatTimestamps() throws {
        // JSON numbers may decode as `Int` or `Double`. Both should land
        // as `TimeInterval` cleanly.
        let json = """
        {"chapters": [
          {"startTime": 0, "title": "A"},
          {"startTime": 252.5, "title": "B"}
        ]}
        """.data(using: .utf8)!
        let chapters = try ChaptersClient.decode(json)
        XCTAssertEqual(chapters[0].startTime, 0)
        XCTAssertEqual(chapters[1].startTime, 252.5)
    }

    func testReturnsEmptyForEmptyChaptersArray() throws {
        let json = #"{"chapters": []}"#.data(using: .utf8)!
        let chapters = try ChaptersClient.decode(json)
        XCTAssertTrue(chapters.isEmpty)
    }

    func testThrowsDecodeErrorOnMissingChaptersKey() {
        let json = "{}".data(using: .utf8)!
        XCTAssertThrowsError(try ChaptersClient.decode(json)) { error in
            guard case ChaptersClient.FetchError.decode = error else {
                return XCTFail("Expected decode error, got \(error)")
            }
        }
    }

    func testThrowsDecodeErrorOnMalformedJSON() {
        let json = "not valid json".data(using: .utf8)!
        XCTAssertThrowsError(try ChaptersClient.decode(json)) { error in
            guard case ChaptersClient.FetchError.decode = error else {
                return XCTFail("Expected decode error, got \(error)")
            }
        }
    }
}
