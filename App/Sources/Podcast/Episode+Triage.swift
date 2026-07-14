import Foundation

// MARK: - TriageDecision
//
// Per-episode decision recorded by the autonomous AI Inbox triage pass.
// One of: `.inbox` (the agent thinks the user should listen to it — surfaced
// on Home with a rationale chip) or `.archived` (silent dismissal — the
// episode stays in the store and remains visible on the show page, but
// drops out of unplayed counts, Continue Listening, and the recent feed
// so the user isn't bothered by it).
//
// "No review" by design: archived episodes carry no rationale because the
// user isn't meant to audit them. Inbox picks always carry a one-line
// rationale, surfaced as the "Because …" line on the pick card.
enum TriageDecision: String, Codable, Sendable, Hashable, CaseIterable {
    case inbox
    case archived
}

extension Episode {
    /// `true` when the agent has marked this episode for the Inbox surface.
    var isInInbox: Bool { triageDecision == .inbox }

    /// `true` when the agent has silently archived this episode. Such
    /// episodes are still in the store (recoverable from the show page)
    /// but should be filtered out of unplayed counts and the recent feed.
    var isTriageArchived: Bool { triageDecision == .archived }

    /// `true` when the episode has not yet been seen by the triage pass.
    /// Drives `InboxTriageService`'s selection of work — only untriaged
    /// episodes need a decision.
    var isUntriaged: Bool { triageDecision == nil }
}
