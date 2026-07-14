import CoreMedia
import Foundation
import Speech

// MARK: - Transcript adapter for SpeechTranscriber results

extension Transcript {
    /// Converts finalized `SpeechTranscriber.Result` objects into a `Transcript`
    /// with `TranscriptSource.onDevice`.
    ///
    /// Each `Result` maps to one `Segment`. Word-level timestamps are extracted
    /// from the `audioTimeRange` attributes on the result's `AttributedString`
    /// when available (requires `timeIndexedProgressiveTranscription` preset).
    static func fromAppleResults(
        _ results: [SpeechTranscriber.Result],
        episodeID: UUID,
        locale: Locale
    ) -> Transcript {
        let segments = results.compactMap { makeSegment(from: $0) }

        return Transcript(
            episodeID: episodeID,
            language: locale.identifier,
            source: .onDevice,
            segments: segments,
            speakers: [],
            generatedAt: Date()
        )
    }

    // MARK: Private helpers

    private static func makeSegment(from result: SpeechTranscriber.Result) -> Segment? {
        let text = String(result.text.characters)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return nil }

        let start = result.range.start.seconds.finiteOrZero
        let end = (result.range.start + result.range.duration).seconds.finiteOrZero

        let words = extractWords(from: result)
        return Segment(
            start: start,
            end: end,
            text: text,
            words: words.isEmpty ? nil : words
        )
    }

    /// Word-level timing extraction from `SpeechTranscriber.Result.text` runs
    /// is gated on the `AttributeScopes.SpeechAttributes` shorthand resolving
    /// against the runtime SDK; that path didn't compile on the iOS 26 SDK
    /// available here, so we return `[]` until the access pattern is nailed
    /// down. Segment-level timing (start/end on the segment itself) is
    /// unaffected — only per-word highlighting falls back to interpolation.
    private static func extractWords(from result: SpeechTranscriber.Result) -> [Word] {
        _ = result
        return []
    }
}

// MARK: - CMTime helpers

private extension Double {
    /// Returns `self` if finite, zero otherwise.
    /// `CMTime.seconds` returns `nan` for invalid/indefinite times.
    var finiteOrZero: Double { isFinite ? self : 0 }
}
