import Foundation

// MARK: - Threading mention

/// One occurrence of a `ThreadingTopic` inside a single episode.
///
/// Mentions are the rows the timeline view renders. Each carries the
/// transcript span (start/end ms) for a `play_episode` deep-link, a
/// short snippet for the row preview, a confidence in the topic
/// classification, and a flag for whether the agent flagged the mention as
/// contradicting another mention of the same topic.
///
/// `confidence` stays a `Double` (0...1) intentionally — the threading
/// pipeline reasons about cluster quality with continuous probabilities,
/// distinct from the wiki's three-band `WikiConfidenceBand` which scores
/// claim-vs-citation alignment.
struct ThreadingMention: Codable, Hashable, Identifiable, Sendable {

    var id: UUID
    var topicID: UUID
    var episodeID: UUID
    /// Inclusive start of the cited transcript span, milliseconds.
    var startMS: Int
    /// Exclusive end of the cited span, milliseconds.
    var endMS: Int
    /// Verbatim transcript snippet around the mention. Intentionally short —
    /// long quotes are a fair-use risk and the timeline only shows a teaser
    /// anyway.
    var snippet: String
    /// Cluster confidence (0...1). Surfaces the dotted-underline /
    /// 50%-opacity treatment in the timeline (UX-09 §4).
    var confidence: Double
    /// `true` when the agent marked this mention as opposing another mention
    /// of the same topic. Drives the amber seam treatment.
    var isContradictory: Bool

    init(
        id: UUID = UUID(),
        topicID: UUID,
        episodeID: UUID,
        startMS: Int,
        endMS: Int,
        snippet: String,
        confidence: Double = 0.7,
        isContradictory: Bool = false
    ) {
        self.id = id
        self.topicID = topicID
        self.episodeID = episodeID
        self.startMS = max(0, startMS)
        self.endMS = max(self.startMS, endMS)
        self.snippet = snippet
        self.confidence = max(0, min(1, confidence))
        self.isContradictory = isContradictory
    }

    /// Compact `mm:ss` (or `h:mm:ss` past an hour) suitable for the timestamp
    /// chip rendered next to each timeline row.
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
}
