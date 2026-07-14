import Foundation

// MARK: - SRTParser

/// Foundation-only SRT parser for podcast transcripts. SRT lacks any standard
/// speaker convention, but in practice publishers prefix the cue text with a
/// speaker label (`Sarah: text`, `[TIM]: text`, `> Tim Ferriss: text`). We
/// recognise the common shapes; anything we can't parse falls through with no
/// speaker assigned.
enum SRTParser {

    enum Error: Swift.Error, Sendable {
        case empty
        case malformedTiming(String)
    }

    static func parse(_ source: String, episodeID: UUID, language: String = "en-US") throws -> Transcript {
        let normalised = source
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalised.isEmpty else { throw Error.empty }

        let blocks = normalised.components(separatedBy: "\n\n")
        var segments: [Segment] = []
        var speakersByLabel: [String: Speaker] = [:]

        for block in blocks {
            let lines = block
                .split(separator: "\n", omittingEmptySubsequences: false)
                .map(String.init)
                .filter { !$0.isEmpty }
            guard lines.count >= 2 else { continue }

            // First line is usually a numeric index; skip it if it's not a
            // timing line. The timing line always contains "-->".
            let timingIdx: Int
            if lines[0].contains("-->") {
                timingIdx = 0
            } else if lines.count > 1 && lines[1].contains("-->") {
                timingIdx = 1
            } else {
                continue
            }
            let (start, end) = try parseTiming(lines[timingIdx])
            let textLines = lines[(timingIdx + 1)...]
            let rawText = textLines.joined(separator: " ")
            let (speakerLabel, cleanText) = extractSpeaker(from: rawText)

            let speakerID: UUID?
            if let label = speakerLabel {
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

            segments.append(
                Segment(
                    start: start,
                    end: end,
                    speakerID: speakerID,
                    text: cleanText,
                    words: nil
                )
            )
        }

        segments.sort { $0.start < $1.start }

        return Transcript(
            episodeID: episodeID,
            language: language,
            source: .publisher,
            segments: segments,
            speakers: Array(speakersByLabel.values),
            generatedAt: Date()
        )
    }

    // MARK: Timing

    /// `HH:MM:SS,mmm --> HH:MM:SS,mmm`. Comma is the SRT decimal mark; we
    /// also accept dots because half the wild files use them.
    static func parseTiming(_ line: String) throws -> (TimeInterval, TimeInterval) {
        let parts = line.components(separatedBy: " --> ")
        guard parts.count == 2 else { throw Error.malformedTiming(line) }
        let start = try parseTimestamp(parts[0])
        let end = try parseTimestamp(parts[1])
        return (start, end)
    }

    static func parseTimestamp(_ value: String) throws -> TimeInterval {
        let cleaned = value.trimmingCharacters(in: .whitespaces)
        let pieces = cleaned.split(separator: ":")
        guard pieces.count == 3 else { throw Error.malformedTiming(value) }
        guard let h = Double(pieces[0]), let m = Double(pieces[1]) else {
            throw Error.malformedTiming(value)
        }
        let s = String(pieces[2]).replacingOccurrences(of: ",", with: ".")
        guard let seconds = Double(s) else { throw Error.malformedTiming(value) }
        return h * 3600 + m * 60 + seconds
    }

    // MARK: Speaker extraction

    /// Recognises the most common SRT speaker conventions:
    ///   - `SPEAKER NAME: text`
    ///   - `[Speaker]: text`
    ///   - `>> Speaker: text`
    /// Returns `(label, text)` with the prefix stripped, or `(nil, original)`.
    static func extractSpeaker(from raw: String) -> (String?, String) {
        var text = raw

        // Strip leading `>>` or `>` chevrons used by some captioners.
        while text.hasPrefix(">") {
            text.removeFirst()
            text = text.drop(while: { $0 == ">" || $0 == " " }).description
        }

        // Bracketed: `[Tim]: ...`
        if text.hasPrefix("["),
           let close = text.firstIndex(of: "]"),
           let colon = text.range(of: ":", range: close..<text.endIndex) {
            let label = String(text[text.index(after: text.startIndex)..<close])
                .trimmingCharacters(in: .whitespaces)
            let rest = String(text[colon.upperBound...]).trimmingCharacters(in: .whitespaces)
            return (label.isEmpty ? nil : label, rest)
        }

        // Plain `Name: rest` — restrict to short prefixes that look like names
        // to avoid eating colons inside body text.
        if let colon = text.firstIndex(of: ":") {
            let label = String(text[..<colon]).trimmingCharacters(in: .whitespaces)
            if isPlausibleSpeakerLabel(label) {
                let rest = String(text[text.index(after: colon)...]).trimmingCharacters(in: .whitespaces)
                return (label, rest)
            }
        }
        return (nil, raw)
    }

    /// 1–4 word, ≤30 chars, all-caps OR Title-Case, no sentence punctuation —
    /// matches "Tim Ferriss", "PETER ATTIA", "Dr. Huberman" but not
    /// "Yeah, well: I think" or "https://example.com".
    static func isPlausibleSpeakerLabel(_ s: String) -> Bool {
        guard !s.isEmpty, s.count <= 30 else { return false }
        if s.contains("//") || s.contains(",") || s.contains("?") { return false }
        let words = s.split(separator: " ")
        guard (1...4).contains(words.count) else { return false }
        for w in words {
            guard let first = w.first, first.isLetter else { return false }
            // Allow titles like "Dr." — letters, dots, hyphens, apostrophes.
            for c in w where !(c.isLetter || c == "." || c == "-" || c == "'") {
                return false
            }
        }
        // Must contain at least one uppercase letter to discourage fragments.
        return s.contains(where: { $0.isUppercase })
    }
}
