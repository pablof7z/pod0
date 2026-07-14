import Foundation

// MARK: - Threading topic

/// A single topic the threading layer surfaces — the noun phrase the user
/// has heard recur across their library.
///
/// One topic clusters every `ThreadingMention` that points at the same
/// underlying concept (e.g. "ketogenic diet"). Slug is the dual-link target
/// shared with the wiki layer; when a topic page exists in the wiki, the
/// slug matches `WikiPage.slug` so the detail sheet can hand off without a
/// second resolution step.
///
/// Counts are denormalised so the list does not have to re-aggregate
/// `ThreadingMention` on every render.
struct ThreadingTopic: Codable, Hashable, Identifiable, Sendable {

    var id: UUID
    /// URL-safe canonical key (matches `WikiPage.normalize(slug:)`).
    var slug: String
    /// Human-readable label rendered in editorial serif.
    var displayName: String
    /// One-paragraph definition the agent compiled. Optional — a freshly
    /// inferred topic may still be awaiting synthesis.
    var definition: String?
    /// Number of distinct episodes the topic has been mentioned in. Drives
    /// the "heard 7x" counter and the threshold gate (UX-09 §7).
    var episodeMentionCount: Int
    /// Number of mentions classified as contradictory pairs. Surfaces the
    /// amber dot on the topic row.
    var contradictionCount: Int
    /// Wall-clock time of the most recent mention. Used to sort the topic
    /// list newest-first.
    var lastMentionedAt: Date?

    init(
        id: UUID = UUID(),
        slug: String,
        displayName: String,
        definition: String? = nil,
        episodeMentionCount: Int = 0,
        contradictionCount: Int = 0,
        lastMentionedAt: Date? = nil
    ) {
        self.id = id
        self.slug = slug
        self.displayName = displayName
        self.definition = definition
        self.episodeMentionCount = max(0, episodeMentionCount)
        self.contradictionCount = max(0, contradictionCount)
        self.lastMentionedAt = lastMentionedAt
    }
}
