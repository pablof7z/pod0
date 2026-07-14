import Foundation

/// MIME-type classification of a publisher-supplied transcript URL, taken from
/// the Podcasting 2.0 `<podcast:transcript>` `type` attribute.
///
/// See `docs/spec/research/transcription-stack.md` §2 for the parser dispatch
/// table. Lane 5 picks an adapter based on this enum, so keep the surface
/// minimal and stable.
enum TranscriptKind: String, Codable, Sendable, Hashable {
    /// `text/vtt` — preferred; supports `<v Speaker>` diarization tags.
    case vtt
    /// `application/x-subrip` — speaker often inlined as `Sarah: ...`.
    case srt
    /// `application/json` — Podcasting 2.0 JSON transcript schema.
    case json
    /// `text/html` — last resort, no usable timestamps.
    case html
    /// `text/plain` — last resort, no timestamps.
    case text

    /// Best-effort classification from a raw `type` attribute string.
    /// Returns `nil` for unknown MIME values so callers can decide to skip.
    static func from(mimeType raw: String?) -> TranscriptKind? {
        guard let raw else { return nil }
        let normalized = raw.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
        switch normalized {
        case "text/vtt", "application/vtt", "vtt":
            return .vtt
        case "application/x-subrip", "application/srt", "text/srt", "srt":
            return .srt
        case "application/json", "application/json+podcastindex.org",
             "application/json; charset=utf-8":
            return .json
        case "text/html", "html":
            return .html
        case "text/plain", "plain":
            return .text
        default:
            // Tolerate parameters like `text/vtt; charset=utf-8`.
            if normalized.hasPrefix("text/vtt") { return .vtt }
            if normalized.hasPrefix("application/json") { return .json }
            if normalized.hasPrefix("text/html") { return .html }
            if normalized.hasPrefix("text/plain") { return .text }
            return nil
        }
    }
}
