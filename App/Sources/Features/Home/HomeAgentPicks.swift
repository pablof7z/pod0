import Foundation

// MARK: - HomeAgentPick

/// One agent-curated pick surfaced in the Home featured section.
struct HomeAgentPick: Identifiable, Equatable, Sendable {
    /// The episode the pick refers to. Stable for `Identifiable`.
    let episodeID: UUID
    var id: UUID { episodeID }
    /// One- or two-sentence reason the pick was selected. The hero pick
    /// gets a richer 2-sentence rationale; secondaries stay terse.
    let rationale: String
    /// 2–3 sentence variant of `rationale` shaped to be *spoken* aloud
    /// (Task 3 — voice-narrated rationale). Empty when the source either
    /// (a) was the heuristic fallback or (b) the LLM omitted the field.
    /// Callers fall back to `rationale` when this is empty.
    let spokenRationale: String
    /// `true` when this is the *hero* pick rendered larger above the rail.
    let isHero: Bool

    init(
        episodeID: UUID,
        rationale: String,
        spokenRationale: String = "",
        isHero: Bool
    ) {
        self.episodeID = episodeID
        self.rationale = rationale
        self.spokenRationale = spokenRationale
        self.isHero = isHero
    }
}

// MARK: - HomeAgentPicksBundle

/// The full set of picks for one Home appearance, plus the source label
/// surfaced in the disclosure ("Picked by your agent" vs the heuristic
/// fallback). The cache key (subscription count + recent-episode
/// fingerprint) is folded in so the agent service can detect material
/// state changes that should invalidate a cached bundle even before the
/// 6-hour TTL has elapsed.
struct HomeAgentPicksBundle: Equatable, Sendable {

    enum Source: String, Sendable {
        case agent
        case fallback
    }

    let picks: [HomeAgentPick]
    let source: Source
    let generatedAt: Date

    var hero: HomeAgentPick? {
        picks.first { $0.isHero }
    }

    var secondaries: [HomeAgentPick] {
        picks.filter { !$0.isHero }
    }

    /// Empty bundle returned when no picks could be derived (no
    /// subscriptions yet, or every show has zero unplayed episodes).
    static let empty = HomeAgentPicksBundle(
        picks: [],
        source: .fallback,
        generatedAt: .distantPast
    )
}
