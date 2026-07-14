import XCTest
@testable import Podcastr

final class SRTParserTests: XCTestCase {

    // MARK: - Happy path

    func testParsesNumberedCues() throws {
        let srt = """
        1
        00:00:00,000 --> 00:00:02,500
        Welcome back to the show.

        2
        00:00:02,500 --> 00:00:06,000
        Today we're talking about ketones.
        """
        let transcript = try SRTParser.parse(srt, episodeID: UUID())
        XCTAssertEqual(transcript.source, .publisher)
        XCTAssertEqual(transcript.segments.count, 2)
        XCTAssertEqual(transcript.segments[0].start, 0.0, accuracy: 0.001)
        XCTAssertEqual(transcript.segments[0].end, 2.5, accuracy: 0.001)
        XCTAssertEqual(transcript.segments[0].text, "Welcome back to the show.")
        XCTAssertEqual(transcript.segments[1].text, "Today we're talking about ketones.")
    }

    func testAcceptsDotDecimalSeparator() throws {
        // Some SRT files use periods instead of commas.
        let srt = """
        1
        00:00:00.000 --> 00:00:02.500
        First cue.
        """
        let transcript = try SRTParser.parse(srt, episodeID: UUID())
        XCTAssertEqual(transcript.segments[0].end, 2.5, accuracy: 0.001)
    }

    // MARK: - Speaker extraction

    func testExtractsTitleCaseSpeakerPrefix() throws {
        let srt = """
        1
        00:00:00,000 --> 00:00:03,000
        Tim Ferriss: So when you talk about metabolic flexibility.

        2
        00:00:03,000 --> 00:00:06,000
        Peter Attia: Right, so the term gets thrown around.
        """
        let transcript = try SRTParser.parse(srt, episodeID: UUID())
        XCTAssertEqual(transcript.speakers.count, 2)
        XCTAssertEqual(transcript.segments[0].text, "So when you talk about metabolic flexibility.")
        XCTAssertEqual(transcript.segments[1].text, "Right, so the term gets thrown around.")
    }

    func testExtractsBracketedSpeakerLabel() throws {
        let srt = """
        1
        00:00:00,000 --> 00:00:03,000
        [Tim]: Hello there.
        """
        let transcript = try SRTParser.parse(srt, episodeID: UUID())
        XCTAssertEqual(transcript.speakers.first?.label, "Tim")
        XCTAssertEqual(transcript.segments[0].text, "Hello there.")
    }

    func testIgnoresSpuriousColon() throws {
        // Long body text with a colon must NOT be treated as a speaker prefix.
        let srt = """
        1
        00:00:00,000 --> 00:00:03,000
        Yeah, well: I think the orthodoxy is changing slowly.
        """
        let transcript = try SRTParser.parse(srt, episodeID: UUID())
        XCTAssertTrue(transcript.speakers.isEmpty)
        XCTAssertEqual(transcript.segments[0].text, "Yeah, well: I think the orthodoxy is changing slowly.")
    }

    // MARK: - Sorting + edge cases

    func testSortsSegmentsByStart() throws {
        let srt = """
        2
        00:00:10,000 --> 00:00:12,000
        Second.

        1
        00:00:00,000 --> 00:00:02,000
        First.
        """
        let transcript = try SRTParser.parse(srt, episodeID: UUID())
        XCTAssertEqual(transcript.segments[0].text, "First.")
        XCTAssertEqual(transcript.segments[1].text, "Second.")
    }

    func testEmptyInputThrows() {
        XCTAssertThrowsError(try SRTParser.parse("   \n\n", episodeID: UUID()))
    }
}
