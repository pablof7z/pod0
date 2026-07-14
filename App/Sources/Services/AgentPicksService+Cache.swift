import Foundation

// MARK: - AgentPicksService + Cache
//
// Per-category cache key, fingerprint, input collection, and the cache
// freshness rule. Lives on its own so the magazine-mode surgery doesn't
// push `AgentPicksService.swift` past the AGENTS.md file-length cap.

/// Cache slot key. `categoryID == nil` is the All-Categories pseudo-
/// category (the brief's "selecting All Categories returns to the
/// existing global behaviour" path).
struct PicksCategoryKey: Hashable, Sendable {
    let categoryID: UUID?
    static let all = PicksCategoryKey(categoryID: nil)
}

/// Cheap stable signature for one category's cache slot. Marking an
/// episode in the category played changes `unplayedCount` and bumps the
/// fingerprint — exactly the case we want to refresh — without
/// invalidating other categories' slots.
struct PicksFingerprint: Equatable, Sendable {
    let subscriptionCount: Int
    let unplayedCount: Int
    let newestUnplayed: Date
}

extension AgentPicksService {

    // MARK: - Cache decision

    func shouldUseCache(now: Date, key: PicksCategoryKey, fingerprint: PicksFingerprint) -> Bool {
        guard let bundle = bundles[key], !bundle.picks.isEmpty else { return false }
        guard let last = fingerprints[key], last == fingerprint else { return false }
        return now.timeIntervalSince(bundle.generatedAt) < Self.cacheTTL
    }

    // MARK: - Fingerprint

    func makeFingerprint(store: AppStateStore, category: PodcastCategory?) -> PicksFingerprint {
        let allowed = category.map { Set($0.subscriptionIDs) }
        let unplayed = store.state.episodes.filter { ep in
            guard !ep.played else { return false }
            if let allowed { return allowed.contains(ep.podcastID) }
            return true
        }
        let newest = unplayed.map(\.pubDate).max() ?? .distantPast
        // Subscription count for fingerprint is the section's count when
        // a category is active — adding/removing a show inside the
        // category bumps the slot, but adding to a different category
        // leaves this section's fingerprint untouched.
        let subscriptionCount: Int
        if let category {
            subscriptionCount = category.subscriptionIDs.count
        } else {
            subscriptionCount = store.state.subscriptions.count
        }
        return PicksFingerprint(
            subscriptionCount: subscriptionCount,
            unplayedCount: unplayed.count,
            newestUnplayed: newest
        )
    }

    // MARK: - Inputs

    func collectInputs(store: AppStateStore, category: PodcastCategory?) -> AgentPicksInputs {
        // Use the cached recent-episodes view when the section is global,
        // otherwise filter the full episode set so the candidate list is
        // bounded by the category's shows. The hot path in production is
        // the global one — the per-category path runs at most once per
        // section per six hours, so the linear filter is fine.
        let allowedSubs: Set<UUID>? = category.map { Set($0.subscriptionIDs) }
        let unplayed: [Episode]
        let inProgress: [Episode]
        if let allowedSubs {
            unplayed = store.state.episodes
                .filter { !$0.played && allowedSubs.contains($0.podcastID) }
                .sorted { $0.pubDate > $1.pubDate }
                .prefix(Self.promptSubscriptionCap)
                .map { $0 }
            inProgress = store.inProgressEpisodes.filter { allowedSubs.contains($0.podcastID) }
        } else {
            unplayed = store.recentEpisodes(limit: Self.promptSubscriptionCap)
            inProgress = store.inProgressEpisodes
        }
        let memories = store.state.agentMemories.filter { !$0.deleted }.prefix(10).map(\.content)
        // Topic hints: when scoped, drop topics whose mentions don't land
        // in the section's shows so the prompt's "Active topics" list
        // doesn't seed picks the user wouldn't expect to see in
        // (e.g.) Learning.
        let scopedTopics = HomeCategoryScope.topicsInCategory(
            topics: store.threadingTopics,
            mentions: store.state.threadingMentions,
            episodes: store.state.episodes,
            allowedSubscriptionIDs: allowedSubs
        )
        let topicNames = scopedTopics.prefix(3).map { $0.displayName }
        let lookup = Dictionary(
            uniqueKeysWithValues: store.state.podcasts.map { ($0.id, $0.title) }
        )
        return AgentPicksInputs(
            unplayed: unplayed,
            inProgress: inProgress,
            subscriptionTitles: lookup,
            memorySnippets: Array(memories),
            topicNames: Array(topicNames)
        )
    }
}
