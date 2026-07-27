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
        return recencySorted(followed)
    }

    /// Podcasts the app knows about but the user does NOT follow, sorted by
    /// the same recency rule as `sortedFollowedPodcastsByRecency`.
    ///
    /// These rows exist because knowing about a podcast is decoupled from
    /// following it: the agent's external-play flow attaches episodes to a
    /// real show without forcing a follow, and unfollowing a show keeps its
    /// podcast row and episodes. Home renders them below the followed shows
    /// so that content stays reachable instead of being stranded in the
    /// store with no surface.
    ///
    /// The Unknown sentinel is excluded — it is an implementation detail of
    /// the external-play fallback, and offering it as a deletable row would
    /// let the user break subsequent external plays.
    var sortedUnfollowedPodcastsByRecency: [Podcast] {
        let followedIDs = Set(state.subscriptions.map(\.podcastID))
        let unfollowed = state.podcasts.filter {
            $0.id != Podcast.unknownID && !followedIDs.contains($0.id)
        }
        return recencySorted(unfollowed)
    }

    /// Orders podcasts by their most-recent-episode `pubDate`, descending.
    /// Podcasts with no known episode sink to the bottom and fall back to
    /// alphabetical order so the list never collapses to a random
    /// arrangement. Per-show recency is read from the precomputed
    /// `episodeIndexesByShow` projection, so the lookup is O(1) per podcast.
    private func recencySorted(_ podcasts: [Podcast]) -> [Podcast] {
        let episodes = state.episodes
        var lookup: [UUID: Date] = [:]
        lookup.reserveCapacity(podcasts.count)
        for podcast in podcasts {
            if let firstIdx = episodeIndexesByShow[podcast.id]?.first,
               episodes.indices.contains(firstIdx) {
                lookup[podcast.id] = episodes[firstIdx].pubDate
            }
        }
        return podcasts.sorted { lhs, rhs in
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
