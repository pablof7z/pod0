import Foundation

// MARK: - VTTParser

/// Foundation-only WebVTT parser tuned for podcast transcripts. Recognises:
///   - Standard cue header `WEBVTT`, optional `NOTE` blocks (skipped).
///   - Optional cue identifier line (skipped).
///   - Timestamp line `HH:MM:SS.mmm --> HH:MM:SS.mmm` (also accepts MM:SS.mmm
///     for mainstream encoders that omit the hour part).
///   - Speaker tags `<v Tim Ferriss>...</v>` ([Podcasting 2.0 convention]).
///
/// What we deliberately drop: cue settings (alignment, position), styling
/// blocks, region blocks. Podcast transcripts use ~none of this.
enum VTTParser {

    enum Error: Swift.Error, Sendable {
        case missingHeader
        case malformedTiming(String)
    }

    static func parse(_ source: String, episodeID: UUID, language: String = "en-US") throws -> Transcript {
        // Normalise line endings; the spec allows CRLF, LF, or a lone CR.
        let normalised = source
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")

        // Split into blocks separated by blank lines.
        let blocks = normalised.components(separatedBy: "\n\n")
        guard let first = blocks.first, first.hasPrefix("WEBVTT") else {
            throw Error.missingHeader
        }

        var segments: [Segment] = []
        var speakersByLabel: [String: Speaker] = [:]

        for block in blocks.dropFirst() {
            let lines = block
                .split(separator: "\n", omittingEmptySubsequences: false)
                .map(String.init)
            guard !lines.isEmpty else { continue }

            // Skip NOTE / STYLE / REGION blocks.
            if let head = lines.first, head.hasPrefix("NOTE") || head.hasPrefix("STYLE") || head.hasPrefix("REGION") {
                continue
            }

            // Find the timing line (first line containing "-->").
            guard let timingIdx = lines.firstIndex(where: { $0.contains("-->") }) else {
                continue
            }
            let (start, end) = try parseTiming(lines[timingIdx])
            let textLines = lines[(timingIdx + 1)...]
                .filter { !$0.isEmpty }
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

        // Sort defensively — most files are ordered, but it's cheap insurance.
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

    /// Parses a timing line into `(start, end)` seconds.
    static func parseTiming(_ line: String) throws -> (TimeInterval, TimeInterval) {
        // Strip cue settings (everything after the second timestamp).
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        let parts = trimmed.components(separatedBy: " --> ")
        guard parts.count >= 2 else {
            throw Error.malformedTiming(line)
        }
        let start = try parseTimestamp(parts[0])
        // The right side may carry cue settings: "00:01:00.000 align:start".
        let rightRaw = parts[1].split(separator: " ").first.map(String.init) ?? parts[1]
        let end = try parseTimestamp(rightRaw)
        return (start, end)
    }

    /// `HH:MM:SS.mmm` or `MM:SS.mmm`.
    static func parseTimestamp(_ value: String) throws -> TimeInterval {
        let pieces = value.split(separator: ":")
        guard pieces.count == 2 || pieces.count == 3 else {
            throw Error.malformedTiming(value)
        }
        var hours: Double = 0
        var minutes: Double = 0
        let secondsRaw: String
        if pieces.count == 3 {
            guard let h = Double(pieces[0]), let m = Double(pieces[1]) else {
                throw Error.malformedTiming(value)
            }
            hours = h
            minutes = m
            secondsRaw = String(pieces[2])
        } else {
            guard let m = Double(pieces[0]) else {
                throw Error.malformedTiming(value)
            }
            minutes = m
            secondsRaw = String(pieces[1])
        }
        guard let seconds = Double(secondsRaw.replacingOccurrences(of: ",", with: ".")) else {
            throw Error.malformedTiming(value)
        }
        return hours * 3600 + minutes * 60 + seconds
    }

    // MARK: Speaker extraction

    /// `<v Speaker Name>text...` → ("Speaker Name", "text...").
    /// Falls back to plain text when no `<v>` tag is present.
    static func extractSpeaker(from text: String) -> (String?, String) {
        guard let openRange = text.range(of: "<v "),
              let closeAngle = text.range(of: ">", range: openRange.upperBound..<text.endIndex)
        else {
            return (nil, stripTags(text))
        }
        let nameRaw = text[openRange.upperBound..<closeAngle.lowerBound]
        let name = nameRaw.trimmingCharacters(in: .whitespaces)
        var rest = String(text[closeAngle.upperBound...])
        if let endTag = rest.range(of: "</v>") {
            rest.removeSubrange(endTag)
        }
        return (name.isEmpty ? nil : name, stripTags(rest).trimmingCharacters(in: .whitespaces))
    }

    /// Strip leftover VTT tags (`<c.classname>`, `<i>`, `<b>`, `<u>`,
    /// `<00:01:23.456>` word timings) without pulling in NSRegularExpression.
    static func stripTags(_ text: String) -> String {
        var out = ""
        out.reserveCapacity(text.count)
        var inTag = false
        for ch in text {
            if ch == "<" { inTag = true; continue }
            if ch == ">" { inTag = false; continue }
            if !inTag { out.append(ch) }
        }
        return out
    }
}
