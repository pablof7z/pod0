import XCTest
@testable import Podcastr

/// Polish-surface coverage for `PlayerTranscriptScrollView`'s active-segment
/// resolver — the binary-search lookup it newly relies on for follow-along
/// row tinting + auto-scroll. The view itself is hard to drive in isolation
/// (it pulls `AppStateStore` via `@Environment` and a `@Bindable PlaybackState`),
/// but the lookup is a pure function on `Transcript` so we can pin its
/// behaviour at the boundaries that matter for the player surface:
///
///  - Before the first segment starts → `nil` (UI keeps inactive styling).
///  - At an exact segment `start` → that segment.
///  - Strictly between cues → the immediately-preceding segment (the speaker
///    just finished — leave their line highlighted, not the next one).
///  - At a segment `end` → that segment.
///  - Past the final segment → the last segment.
///  - Empty transcript → `nil` (guards the new
///    `transcript.segments.isEmpty` route to the empty-state prompt).
@MainActor
final class PlayerTranscriptSegmentLookupTests: XCTestCase {

    // MARK: - Fixtures

    /// Three back-to-back cues with a 1s gap between cue 2 and cue 3, so we
    /// can probe both the "between cues" and "exact boundary" cases.
    ///
    ///   cue1: 0...4
    ///   cue2: 4...8
    ///   gap:  8...9
    ///   cue3: 9...13
    private func makeTranscript() -> Transcript {
        let speaker = Speaker(label: "host")
        return Transcript(
            episodeID: UUID(),
            language: "en-US",
            source: .publisher,
            segments: [
                Segment(start: 0, end: 4, speakerID: speaker.id, text: "Cue one."),
                Segment(start: 4, end: 8, speakerID: speaker.id, text: "Cue two."),
                Segment(start: 9, end: 13, speakerID: speaker.id, text: "Cue three.")
            ],
            speakers: [speaker]
        )
    }

    // MARK: - Tests

    func testSegmentLookupBeforeFirstStartReturnsNil() {
        let transcript = makeTranscript()
        XCTAssertNil(transcript.segment(at: -1))
    }

    func testSegmentLookupAtExactStartReturnsThatSegment() {
        let transcript = makeTranscript()
        let resolved = transcript.segment(at: 4)
        XCTAssertEqual(resolved?.text, "Cue two.")
    }

    func testSegmentLookupAtSegmentEndReturnsThatSegment() {
        let transcript = makeTranscript()
        let resolved = transcript.segment(at: 8)
        // 8 falls within cue2's [4...8] inclusive range — the binary search
        // returns it directly, *not* the next cue.
        XCTAssertEqual(resolved?.text, "Cue two.")
    }

    func testSegmentLookupInGapReturnsPriorSegment() {
        let transcript = makeTranscript()
        // 8.5 sits in the silent gap between cue2 (ends at 8) and cue3
        // (starts at 9). The active row should remain cue2 — the speaker
        // just finished.
        let resolved = transcript.segment(at: 8.5)
        XCTAssertEqual(resolved?.text, "Cue two.")
    }

    func testSegmentLookupPastFinalEndReturnsLastSegment() {
        let transcript = makeTranscript()
        let resolved = transcript.segment(at: 999)
        XCTAssertEqual(resolved?.text, "Cue three.")
    }

    func testSegmentLookupOnEmptyTranscriptReturnsNil() {
        // Guards the `synced(transcript:)` `isEmpty` route — if a `.ready`
        // transcript ever happens to have zero segments (corrupt parse,
        // future ingestor bug), the active-segment resolver must not crash
        // and must return `nil` so the UI falls back to the empty-state.
        let empty = Transcript(
            episodeID: UUID(),
            language: "en-US",
            source: .publisher,
            segments: [],
            speakers: []
        )
        XCTAssertNil(empty.segment(at: 0))
        XCTAssertNil(empty.segment(at: 12.5))
    }
}
