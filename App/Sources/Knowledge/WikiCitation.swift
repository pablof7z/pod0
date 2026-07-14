import Foundation

// MARK: - Citation

/// A single point of provenance for a synthesized claim in a `WikiPage`.
///
/// Every claim in a wiki page must trace back to a real transcript span.
/// The citation pairs an episode reference with the exact millisecond range
/// quoted, plus a short verbatim snippet (≤125 chars per the llm-wiki ethos)
/// the user (and the verification pass) can compare against.
///
/// - Note: `quoteSnippet` is intentionally short — long quotes are a fair-use
///   risk and almost always indicate the synthesis layer is paraphrasing
///   rather than reasoning. The 125-char ceiling mirrors `nvk/llm-wiki`.
struct WikiCitation: Codable, Hashable, Identifiable, Sendable {

    /// Per-citation identifier — stable across re-renders so UI can keep
    /// scroll position when a page is regenerated and a citation persists.
    var id: UUID

    /// The episode this citation points into.
    var episodeID: UUID

    /// Inclusive start of the cited span, in milliseconds since the start
    /// of the audio. Player deep-link targets land on this timestamp.
    var startMS: Int

    /// Exclusive end of the cited span, in milliseconds.
    var endMS: Int

    /// A verbatim snippet from the transcript at the cited span. Hard-capped
    /// to `WikiCitation.maxSnippetLength` characters at construction time.
    var quoteSnippet: String

    /// Optional speaker label (from diarization). `nil` when speaker
    /// confidence falls below the diarization threshold — in that case the
    /// claim should attribute "the show," not a named person.
    var speaker: String?

    /// Confidence band the verifier assigned to this citation after
    /// matching the synthesized claim against the cited span.
    var verificationConfidence: WikiConfidenceBand

    // MARK: - Constants

    /// Hard cap on `quoteSnippet` length. Mirrors the 125-char fair-use
    /// ceiling adopted by `nvk/llm-wiki` for ingested raw spans.
    static let maxSnippetLength: Int = 125

    // MARK: - Init

    init(
        id: UUID = UUID(),
        episodeID: UUID,
        startMS: Int,
        endMS: Int,
        quoteSnippet: String,
        speaker: String? = nil,
        verificationConfidence: WikiConfidenceBand = .medium
    ) {
        self.id = id
        self.episodeID = episodeID
        self.startMS = max(0, startMS)
        self.endMS = max(self.startMS, endMS)
        self.quoteSnippet = WikiCitation.clamp(quoteSnippet)
        self.speaker = speaker
        self.verificationConfidence = verificationConfidence
    }

    // MARK: - Derived

    /// Duration of the cited span in milliseconds. Always ≥ 0.
    var durationMS: Int { max(0, endMS - startMS) }

    /// Compact `mm:ss` formatting suitable for the citation chip.
    var formattedTimestamp: String {
        let totalSeconds = startMS / 1_000
        let hours = totalSeconds / 3_600
        let minutes = (totalSeconds % 3_600) / 60
        let seconds = totalSeconds % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, seconds)
        }
        return String(format: "%d:%02d", minutes, seconds)
    }

    // MARK: - Helpers

    // MARK: - Codable (back-compat)

    private enum CodingKeys: String, CodingKey {
        case id, episodeID, startMS, endMS
        case quoteSnippet, speaker, verificationConfidence
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            id: try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID(),
            episodeID: try c.decode(UUID.self, forKey: .episodeID),
            startMS: try c.decodeIfPresent(Int.self, forKey: .startMS) ?? 0,
            endMS: try c.decodeIfPresent(Int.self, forKey: .endMS) ?? 0,
            quoteSnippet: try c.decodeIfPresent(String.self, forKey: .quoteSnippet) ?? "",
            speaker: try c.decodeIfPresent(String.self, forKey: .speaker),
            verificationConfidence: try c.decodeIfPresent(WikiConfidenceBand.self, forKey: .verificationConfidence) ?? .medium
        )
    }

    /// Truncates `value` to fit `maxSnippetLength`. Trims whitespace at the
    /// boundary and appends an ellipsis when shortened.
    static func clamp(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count > maxSnippetLength else { return trimmed }
        let cutoff = trimmed.index(trimmed.startIndex, offsetBy: maxSnippetLength - 1)
        let prefix = String(trimmed[..<cutoff])
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return prefix + "…"
    }
}

// MARK: - Confidence band

/// Three-band confidence scoring used by both citations and synthesized
/// claims. Mirrors the `high|medium|low` enum in the llm-wiki frontmatter
/// schema. Color and accessibility mapping live in the UI layer.
enum WikiConfidenceBand: String, Codable, CaseIterable, Sendable {
    case high
    case medium
    case low

    /// Shortform label rendered next to claim margins in the editorial UI.
    var label: String {
        switch self {
        case .high: "high"
        case .medium: "medium"
        case .low: "low"
        }
    }

    /// VoiceOver-readable phrasing for accessibility values.
    var accessibilityValue: String {
        switch self {
        case .high: "high evidence"
        case .medium: "medium evidence"
        case .low: "low evidence"
        }
    }
}
