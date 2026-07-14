import XCTest
@testable import Podcastr

/// Pins the pure pieces of `ClipBoundaryResolver`: parsing, validation,
/// speaker resolution. The LLM round-trip itself is exercised via a stubbed
/// `WikiOpenRouterClient` so the resolver's full path is testable without a
/// real provider key.
@MainActor
final class ClipBoundaryResolverTests: XCTestCase {

    // MARK: - Fixture

    private func makeTranscript() -> Transcript {
        let host = Speaker(label: "host", displayName: "Host")
        let guest = Speaker(label: "guest", displayName: "Guest")
        let segs: [Segment] = [
            Segment(start: 100, end: 110, speakerID: host.id,  text: "Welcome back to the show."),
            Segment(start: 110, end: 122, speakerID: host.id,  text: "Today we are talking about metabolic flexibility."),
            Segment(start: 122, end: 138, speakerID: guest.id, text: "So the body's ability to switch substrate is the real measure."),
            Segment(start: 138, end: 150, speakerID: guest.id, text: "Without that, you are stuck burning whatever is available."),
            Segment(start: 150, end: 160, speakerID: host.id,  text: "That's a great point."),
        ]
        return Transcript(
            episodeID: UUID(),
            language: "en-US",
            source: .scribeV1,
            segments: segs,
            speakers: [host, guest]
        )
    }

    // MARK: - Parse: happy path

    func testParseHappyPathReturnsValidatedBoundaries() {
        let raw = """
        {"startSeconds": 122, "endSeconds": 150,
         "quotedText": "So the body's ability to switch substrate is the real measure. Without that, you are stuck burning whatever is available.",
         "speakerLabel": "Guest"}
        """
        let resolved = ClipBoundaryResolver.shared.parse(raw, transcript: makeTranscript())
        XCTAssertNotNil(resolved)
        XCTAssertEqual(resolved?.startSeconds, 122)
        XCTAssertEqual(resolved?.endSeconds, 150)
        XCTAssertEqual(resolved?.speakerLabel, "Guest")
        XCTAssertNotNil(resolved?.speakerID, "single-speaker span should resolve to guest UUID")
    }

    // MARK: - Parse: clamping & validation

    func testParseClampsToTranscriptSpan() {
        // LLM hallucinates bounds outside the transcript window.
        let raw = """
        {"startSeconds": 50, "endSeconds": 9999, "quotedText": "x"}
        """
        let resolved = ClipBoundaryResolver.shared.parse(raw, transcript: makeTranscript())
        XCTAssertEqual(resolved?.startSeconds, 100, "should clamp to first segment start")
        XCTAssertEqual(resolved?.endSeconds, 160, "should clamp to last segment end")
    }

    func testParseRejectsZeroOrNegativeRange() {
        let raw = """
        {"startSeconds": 130, "endSeconds": 130, "quotedText": "x"}
        """
        XCTAssertNil(ClipBoundaryResolver.shared.parse(raw, transcript: makeTranscript()))

        let swapped = """
        {"startSeconds": 140, "endSeconds": 120, "quotedText": "x"}
        """
        XCTAssertNil(ClipBoundaryResolver.shared.parse(swapped, transcript: makeTranscript()))
    }

    func testParseRejectsMalformedJSON() {
        XCTAssertNil(ClipBoundaryResolver.shared.parse("not json", transcript: makeTranscript()))
        XCTAssertNil(ClipBoundaryResolver.shared.parse("{\"foo\":1}", transcript: makeTranscript()))
    }

    // MARK: - Parse: speaker resolution

    func testMixedSpeakerSpanLeavesSpeakerIDNil() {
        // [110, 138] is split roughly 50/50 between host and guest — below
        // the 65% majority threshold, so speakerID should be nil.
        let raw = """
        {"startSeconds": 110, "endSeconds": 138, "quotedText": "..."}
        """
        let resolved = ClipBoundaryResolver.shared.parse(raw, transcript: makeTranscript())
        XCTAssertNotNil(resolved)
        XCTAssertNil(resolved?.speakerID, "mixed-speaker span should not have a single speakerID")
    }

    // MARK: - Parse: fallback text

    func testEmptyQuotedTextFallsBackToSegmentJoin() {
        let raw = """
        {"startSeconds": 122, "endSeconds": 150, "quotedText": ""}
        """
        let resolved = ClipBoundaryResolver.shared.parse(raw, transcript: makeTranscript())
        XCTAssertEqual(
            resolved?.quotedText,
            "So the body's ability to switch substrate is the real measure. Without that, you are stuck burning whatever is available."
        )
    }

    // MARK: - Resolve with stubbed client

    func testResolveBoundariesUsesStubbedClient() async {
        let resolver = ClipBoundaryResolver.shared
        let original = resolver.clientFactory
        defer { resolver.clientFactory = original }

        let stub = """
        {"startSeconds": 122, "endSeconds": 150,
         "quotedText": "So the body's ability to switch substrate is the real measure.",
         "speakerLabel": "Guest"}
        """
        resolver.clientFactory = { _ in WikiOpenRouterClient.stubbed(json: stub) }

        let resolved = await resolver.resolveBoundaries(
            transcript: makeTranscript(),
            playheadSeconds: 145, // tap a few seconds after the interesting bit
            intent: .quote,
            modelID: "openai/gpt-4o-mini"
        )
        XCTAssertNotNil(resolved)
        XCTAssertEqual(resolved?.startSeconds, 122)
        XCTAssertEqual(resolved?.endSeconds, 150)
    }

    func testResolveReturnsNilWhenClientFactoryReturnsNil() async {
        let resolver = ClipBoundaryResolver.shared
        let original = resolver.clientFactory
        defer { resolver.clientFactory = original }
        resolver.clientFactory = { _ in nil }

        let resolved = await resolver.resolveBoundaries(
            transcript: makeTranscript(),
            playheadSeconds: 145,
            intent: .clip,
            modelID: "openai/gpt-4o-mini"
        )
        XCTAssertNil(resolved)
    }
}
