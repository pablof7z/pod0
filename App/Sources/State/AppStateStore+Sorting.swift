import Foundation

// MARK: - Followed-podcast sorting (recency)

extension AppStateStore {

    /// Podcasts the user follows, sorted by their most-recent-episode
    /// `pubDate`, descending.
    ///
    /// Designed for the merged Home subscription list — the user wants to see
    /// the feed that just published an episode at the top, not the one whose
    /// title happens to start with "A". Followed podcasts with no known
    /// episode yet (fresh import, before the first feed fetch) sink to the
    /// bottom and fall back to alphabetical order so the list never
    /// collapses to a random arrangement.
    ///
    /// O(N log N) on the followed-podcast count. Per-show recency is read
    /// from the precomputed `episodeIndexesByShow` projection — `.first` of
    /// that array is the newest-pubDate episode index, so the recency
    /// lookup is O(1) per podcast.
    ///
    /// Synthetic podcasts (Agent Generated, Unknown) are excluded by virtue
    /// of having no `PodcastSubscription` row in the new model — they're
    /// `Podcast`-only and never appear in the user's subscription list.
    var sortedFollowedPodcastsByRecency: [Podcast] {
        let podcastByID = Dictionary(uniqueKeysWithValues: state.podcasts.map { ($0.id, $0) })
        let followed = state.subscriptions.compactMap { podcastByID[$0.podcastID] }
            .filter { $0.kind == .rss }
        let episodes = state.episodes
        var lookup: [UUID: Date] = [:]
        lookup.reserveCapacity(followed.count)
        for podcast in followed {
            if let firstIdx = episodeIndexesByShow[podcast.id]?.first,
               episodes.indices.contains(firstIdx) {
                lookup[podcast.id] = episodes[firstIdx].pubDate
            }
        }
        return followed.sorted { lhs, rhs in
            switch (lookup[lhs.id], lookup[rhs.id]) {
            case let (l?, r?):
                if l == r {
                    return lhs.title.localizedCaseInsensitiveCompare(rhs.title) == .orderedAscending
                }
                return l > r
            case (.some, .none):
                return true
            case (.none, .some):
                return false
            case (.none, .none):
                return lhs.title.localizedCaseInsensitiveCompare(rhs.title) == .orderedAscending
            }
        }
    }

    /// Most-recent episode for the given `podcastID`, or `nil` when the
    /// podcast has no episodes yet.
    func mostRecentEpisode(forPodcast podcastID: UUID) -> Episode? {
        guard let firstIdx = episodeIndexesByShow[podcastID]?.first,
              state.episodes.indices.contains(firstIdx) else { return nil }
        return state.episodes[firstIdx]
    }
}
