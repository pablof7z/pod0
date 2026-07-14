import Foundation

// MARK: - HomeCategoryScope
//
// Pure derivation helpers that narrow the Home featured surface to a single
// category. Lifted out of `HomeView` so the magazine-mode behaviour
// (resume rail, dateline counts, threaded-today, agent picks scope) is
// testable without a SwiftUI environment or a live store.
//
// All accessors take an optional `allowedSubscriptionIDs: Set<UUID>?`. A
// `nil` value means "no category active — return the global view"; a
// non-`nil` empty set means "category active but contains no shows" and
// every accessor returns an empty result. That asymmetry preserves the
// "All Categories" pseudo-category path the brief explicitly asks us not
// to break.

enum HomeCategoryScope {

    /// Filter `episodes` (which may be the in-progress rail or the recent
    /// feed) by the active category's subscription set. Pass `nil` to
    /// return `episodes` untouched.
    static func episodesInCategory(
        _ episodes: [Episode],
        allowedSubscriptionIDs: Set<UUID>?
    ) -> [Episode] {
        guard let allowed = allowedSubscriptionIDs else { return episodes }
        return episodes.filter { allowed.contains($0.podcastID) }
    }

    /// Filter `topics` to those whose mentions land in episodes from the
    /// allowed subscription set. Used by the dateline so a contradiction
    /// in a topic that doesn't appear in the current section's shows
    /// stops bumping the `CONTRADICTION` count when the user switches
    /// categories. Returns `topics` untouched when `allowedSubscriptionIDs`
    /// is `nil` (global / All-Categories path).
    static func topicsInCategory(
        topics: [ThreadingTopic],
        mentions: [ThreadingMention],
        episodes: [Episode],
        allowedSubscriptionIDs: Set<UUID>?
    ) -> [ThreadingTopic] {
        guard let allowed = allowedSubscriptionIDs else { return topics }
        let allowedEpisodeIDs: Set<UUID> = Set(
            episodes.filter { allowed.contains($0.podcastID) }.map(\.id)
        )
        guard !allowedEpisodeIDs.isEmpty else { return [] }
        var topicIDsInScope: Set<UUID> = []
        for mention in mentions where allowedEpisodeIDs.contains(mention.episodeID) {
            topicIDsInScope.insert(mention.topicID)
        }
        return topics.filter { topicIDsInScope.contains($0.id) }
    }
}
