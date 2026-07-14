import Foundation

// MARK: - BriefingRequest

/// A user-initiated request to compose a new briefing.
///
/// `scope` and `length` mirror the Compose surface's puck + chips (UX-08 §3).
/// `style` selects between built-in prompt templates in `BriefingPrompts`.
/// `requestedAt` anchors "this morning" / "this week" filtering inside the
/// composer's RAG step.
struct BriefingRequest: Codable, Sendable, Hashable, Identifiable {
    /// Stable identifier that follows the briefing through the pipeline and
    /// becomes the on-disk filename for both the script JSON and stitched .m4a.
    var id: UUID

    /// What slice of the corpus to draw from.
    var scope: BriefingScope

    /// Target length expressed as the puck's discrete stops.
    var length: BriefingLength

    /// Which prompt template to render.
    var style: BriefingStyle

    /// Optional freeform user query (the *"Brief me on…"* field). When set,
    /// the composer will pass this to the LLM in addition to the chosen style.
    var freeformQuery: String?

    /// When the user pressed *Compose*. Used as the "now" reference.
    var requestedAt: Date

    init(
        id: UUID = UUID(),
        scope: BriefingScope = .mySubscriptions,
        length: BriefingLength = .medium,
        style: BriefingStyle = .morning,
        freeformQuery: String? = nil,
        requestedAt: Date = Date()
    ) {
        self.id = id
        self.scope = scope
        self.length = length
        self.style = style
        self.freeformQuery = freeformQuery
        self.requestedAt = requestedAt
    }
}

// MARK: - Supporting enums

/// What slice of the user's library a briefing should draw from.
enum BriefingScope: String, Codable, Sendable, Hashable, CaseIterable {
    case mySubscriptions
    case thisShow
    case thisTopic
    case thisWeek

    var displayName: String {
        switch self {
        case .mySubscriptions: "my subscriptions"
        case .thisShow:        "this show"
        case .thisTopic:       "this topic"
        case .thisWeek:        "this week"
        }
    }
}

/// Discrete length puck stops from the spec (3 / 8 / 15 / 25 min).
enum BriefingLength: String, Codable, Sendable, Hashable, CaseIterable {
    case quick      // 3 min
    case medium     // 8 min
    case extended   // 15 min
    case deepDive   // 25 min

    /// Target length in seconds, used to budget segment durations.
    var targetSeconds: TimeInterval {
        switch self {
        case .quick:    180
        case .medium:   480
        case .extended: 900
        case .deepDive: 1500
        }
    }

    /// Display label used by the puck and library rows.
    var displayLabel: String {
        switch self {
        case .quick:    "3 min"
        case .medium:   "8 min"
        case .extended: "15 min"
        case .deepDive: "25 min"
        }
    }
}

/// The four built-in script templates from UX-08.
enum BriefingStyle: String, Codable, Sendable, Hashable, CaseIterable {
    case morning            // Daily briefing — "Tuesday briefing — 8 min"
    case weeklyTLDR         // Weekly TLDR
    case catchUpOnShow      // "Catch me up on Lex"
    case topicAcrossLibrary // "Brief me on Y across my library"

    var displayLabel: String {
        switch self {
        case .morning:            "Daily briefing"
        case .weeklyTLDR:         "Weekly TLDR"
        case .catchUpOnShow:      "Catch up on…"
        case .topicAcrossLibrary: "Topic deep-dive"
        }
    }
}
