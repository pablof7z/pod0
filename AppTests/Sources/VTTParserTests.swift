import XCTest
@testable import Podcastr

final class VTTParserTests: XCTestCase {

    // MARK: - Happy path

    func testParsesSimpleCues() throws {
        let vtt = """
        WEBVTT

        00:00:00.000 --> 00:00:02.500
        Welcome back to the show.

        00:00:02.500 --> 00:00:06.000
        Today we're talking about ketones.
        """
        let episodeID = UUID()
        let transcript = try VTTParser.parse(vtt, episodeID: episodeID)

        XCTAssertEqual(transcript.episodeID, episodeID)
        XCTAssertEqual(transcript.source, .publisher)
        XCTAssertEqual(transcript.segments.count, 2)
        XCTAssertEqual(transcript.segments[0].start, 0.0, accuracy: 0.001)
        XCTAssertEqual(transcript.segments[0].end, 2.5, accuracy: 0.001)
        XCTAssertEqual(transcript.segments[0].text, "Welcome back to the show.")
        XCTAssertEqual(transcript.segments[1].text, "Today we're talking about ketones.")
        XCTAssertTrue(transcript.speakers.isEmpty)
    }

    func testExtractsSpeakerFromVTag() throws {
        let vtt = """
        WEBVTT

        00:00:00.000 --> 00:00:03.000
        <v Tim Ferriss>So when you talk about metabolic flexibility…</v>

        00:00:03.000 --> 00:00:06.000
        <v Peter Attia>Right, so the term gets thrown around.</v>
        """
        let transcript = try VTTParser.parse(vtt, episodeID: UUID())

        XCTAssertEqual(transcript.segments.count, 2)
        XCTAssertEqual(transcript.segments[0].text, "So when you talk about metabolic flexibility…")
        XCTAssertEqual(transcript.segments[1].text, "Right, so the term gets thrown around.")
        XCTAssertEqual(transcript.speakers.count, 2)
        let labels = Set(transcript.speakers.map(\.label))
        XCTAssertEqual(labels, ["Tim Ferriss", "Peter Attia"])

        // Speaker IDs should be stable within a single transcript — the same
        // speaker tag must point at the same UUID across cues.
        let firstID = transcript.segments[0].speakerID
        let lastID = transcript.segments[1].speakerID
        XCTAssertNotNil(firstID)
        XCTAssertNotNil(lastID)
        XCTAssertNotEqual(firstID, lastID)
    }

    func testReusesSpeakerIDAcrossCues() throws {
        let vtt = """
        WEBVTT

        00:00:00.000 --> 00:00:02.000
        <v Tim Ferriss>One.</v>

        00:00:02.000 --> 00:00:04.000
        <v Tim Ferriss>Two.</v>
        """
        let transcript = try VTTParser.parse(vtt, episodeID: UUID())
        XCTAssertEqual(transcript.speakers.count, 1)
        XCTAssertEqual(transcript.segments[0].speakerID, transcript.segments[1].speakerID)
    }

    // MARK: - Edge cases

    func testHandlesShortMMSSTimestamps() throws {
        // Some encoders omit the hour part on cues < 1 hour.
        let vtt = """
        WEBVTT

        00:30.000 --> 00:35.000
        Short form.
        """
        let transcript = try VTTParser.parse(vtt, episodeID: UUID())
        XCTAssertEqual(transcript.segments.count, 1)
        XCTAssertEqual(transcript.segments[0].start, 30.0, accuracy: 0.001)
        XCTAssertEqual(transcript.segments[0].end, 35.0, accuracy: 0.001)
    }

    func testIgnoresNoteAndStyleBlocks() throws {
        let vtt = """
        WEBVTT

        NOTE
        This is a comment block we should drop.

        STYLE
        ::cue { color: red; }

        00:00:00.000 --> 00:00:02.000
        Real cue.
        """
        let transcript = try VTTParser.parse(vtt, episodeID: UUID())
        XCTAssertEqual(transcript.segments.count, 1)
        XCTAssertEqual(transcript.segments[0].text, "Real cue.")
    }

    func testStripsInlineFormattingTags() throws {
        let vtt = """
        WEBVTT

        00:00:00.000 --> 00:00:02.000
        <v Sarah>This is <b>important</b> stuff.</v>
        """
        let transcript = try VTTParser.parse(vtt, episodeID: UUID())
        XCTAssertEqual(transcript.segments[0].text, "This is important stuff.")
    }

    func testSegmentsAreSortedByStart() throws {
        let vtt = """
        WEBVTT

        00:00:10.000 --> 00:00:12.000
        Second.

        00:00:00.000 --> 00:00:02.000
        First.
        """
        let transcript = try VTTParser.parse(vtt, episodeID: UUID())
        XCTAssertEqual(transcript.segments[0].text, "First.")
        XCTAssertEqual(transcript.segments[1].text, "Second.")
    }

    // MARK: - Error paths

    func testThrowsWhenHeaderMissing() {
        let bad = """
        00:00:00.000 --> 00:00:02.000
        Missing header.
        """
        XCTAssertThrowsError(try VTTParser.parse(bad, episodeID: UUID()))
    }
}
