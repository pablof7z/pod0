import Foundation

// MARK: - PodcastingTranscriptJSONParser

/// Parses the [Podcasting 2.0 JSON transcript format] into our `Transcript`.
///
/// The on-the-wire shape is:
/// ```
/// {
///   "version": "1.0.0",
///   "segments": [
///     { "speaker": "Tim",
///       "startTime": 0.0,
///       "endTime": 3.4,
///       "body": "Welcome back to the show." }
///   ]
/// }
/// ```
/// Some publishers also emit per-word arrays. We accept either an array of
/// `{ word, startTime, endTime }` under a `"words"` key, or a flat segment
/// list (the common case).
enum PodcastingTranscriptJSONParser {

    enum Error: Swift.Error, Sendable {
        case invalidJSON
        case missingSegments
    }

    static func parse(_ data: Data, episodeID: UUID, language: String = "en-US") throws -> Transcript {
        guard
            let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { throw Error.invalidJSON }
        guard let rawSegments = object["segments"] as? [[String: Any]] else {
            throw Error.missingSegments
        }

        var segments: [Segment] = []
        var speakersByLabel: [String: Speaker] = [:]

        for raw in rawSegments {
            guard
                let body = (raw["body"] as? String) ?? (raw["text"] as? String),
                let start = doubleValue(raw["startTime"]) ?? doubleValue(raw["start"]),
                let end = doubleValue(raw["endTime"]) ?? doubleValue(raw["end"])
            else { continue }

            let speakerLabel = (raw["speaker"] as? String)?.trimmed
            let speakerID: UUID?
            if let label = speakerLabel, !label.isEmpty {
                if let existing = speakersByLabel[label] {
                    speakerID = existing.id
                } else {
                    let new = Speaker(label: label, displayName: label)
                    speakersByLabel[label] = new
                    speakerID = new.id
                }
            } else {
                speakerID = nil
            }

            let words: [Word]?
            if let rawWords = raw["words"] as? [[String: Any]] {
                words = rawWords.compactMap { w -> Word? in
                    guard
                        let wText = (w["word"] as? String) ?? (w["text"] as? String),
                        let wStart = doubleValue(w["startTime"]) ?? doubleValue(w["start"]),
                        let wEnd = doubleValue(w["endTime"]) ?? doubleValue(w["end"])
                    else { return nil }
                    return Word(start: wStart, end: wEnd, text: wText)
                }
            } else {
                words = nil
            }

            segments.append(
                Segment(
                    start: start,
                    end: end,
                    speakerID: speakerID,
                    text: body,
                    words: words
                )
            )
        }

        segments.sort { $0.start < $1.start }

        let resolvedLanguage = (object["language"] as? String) ?? language

        return Transcript(
            episodeID: episodeID,
            language: resolvedLanguage,
            source: .publisher,
            segments: segments,
            speakers: Array(speakersByLabel.values),
            generatedAt: Date()
        )
    }

    /// JSON numbers in Foundation can come back as `NSNumber`/`Double`/`Int`;
    /// some publishers stringify them. Accept all three.
    private static func doubleValue(_ any: Any?) -> Double? {
        if let d = any as? Double { return d }
        if let i = any as? Int { return Double(i) }
        if let n = any as? NSNumber { return n.doubleValue }
        if let s = any as? String { return Double(s) }
        return nil
    }
}

