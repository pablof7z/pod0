// PodcastAgentContextTypes.generated.swift
// Hand-maintained mirror of the Rust agent-context projection types.
// Split out of `PodcastUpdate.generated.swift` to keep that file under the
// 500-line hard limit. Keep camelCase in sync with snake_case Rust source —
// `.convertFromSnakeCase` handles the key mapping.
// Source of truth: apps/nmp-app-podcast/src/ffi/projections/agent_context.rs

import Foundation

/// Agent-prompt inventory context. Mirrors
/// `ffi::projections::AgentContextSnapshot`. The kernel performs all
/// selection / ordering / capping; `AgentPrompt` only renders the strings.
struct AgentContextSnapshot: Equatable {
    /// Subscribed-show titles, already sorted + capped by the kernel.
    var subscriptions: [String] = []
    /// Followed-show count *before* the cap (drives the "(N)" header and the
    /// "…and N more" suffix).
    var subscriptionsTotal: Int = 0
    /// In-progress episodes (started, not finished, not archived), newest-first.
    var inProgress: [AgentContextEpisode] = []
    /// Recent unplayed episodes inside the recency window, newest-first.
    var recentUnplayed: [AgentContextEpisode] = []
    /// Recency-window width (days) the kernel applied to `recentUnplayed`.
    var recentWindowDays: Int = 0
}

/// One episode row in `AgentContextSnapshot`. Mirrors
/// `ffi::projections::AgentContextEpisode`. Carries the resolved show title
/// so the renderer needs no second lookup.
struct AgentContextEpisode: Equatable {
    var title: String = ""
    var showTitle: String = ""
}

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]` on the
// collection fields (omit when empty). `decodeIfPresent` with explicit
// fallbacks keeps the decoder forward- and backward-compatible.

extension AgentContextSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        subscriptions = try c.decodeIfPresent([String].self, forKey: .subscriptions) ?? []
        subscriptionsTotal = try c.decodeIfPresent(Int.self, forKey: .subscriptionsTotal) ?? 0
        inProgress = try c.decodeIfPresent([AgentContextEpisode].self, forKey: .inProgress) ?? []
        recentUnplayed = try c.decodeIfPresent([AgentContextEpisode].self, forKey: .recentUnplayed) ?? []
        recentWindowDays = try c.decodeIfPresent(Int.self, forKey: .recentWindowDays) ?? 0
    }
}

extension AgentContextEpisode: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
        showTitle = try c.decodeIfPresent(String.self, forKey: .showTitle) ?? ""
    }
}
