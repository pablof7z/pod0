import Foundation

// MARK: - Episodes

extension AppStateStore {

    // MARK: - Reads

    /// Returns the live episode record matching `id`, or `nil` when not found.
    func episode(id: UUID) -> Episode? {
        if let index = episodeIndexByID[id], state.episodes.indices.contains(index),
           state.episodes[index].id == id {
            return state.episodes[index]
        }
        // Cold safety path for a structurally unusual projection replacement
        // that preserved the array's cheap didSet fingerprint.
        return state.episodes.first { $0.id == id }
    }

    /// Episodes belonging to the given podcast, newest publish-date first.
    ///
    /// O(1) lookup against `episodeIndexesByShow` plus an O(K) position-cache fold
    /// (K = pending position writes, typically ≤ 1). Was O(N) filter + O(N
    /// log N) sort, called from `ShowDetailView`'s body for every render —
    /// 2,853 episodes for "The Daily" alone.
    func episodes(forPodcast id: UUID) -> [Episode] {
        episodesForShowView(id)
    }

    /// Episodes the user has started but not finished, ordered by most recent
    /// activity. "Started" is `playbackPosition > 0`. "Finished" is `played`.
    /// Used by the Home tab's in-progress carousel.
    ///
    /// Backed by `inProgressEpisodesCached`. The read-side helper folds the
    /// position-debounce cache so an episode whose first tick hasn't flushed
    /// yet still surfaces here.
    var inProgressEpisodes: [Episode] {
        inProgressEpisodesView()
    }

    /// Recently published, unplayed episodes across all subscriptions.
    /// Used by the Home tab's "new" feed.
    ///
    /// Backed by `recentEpisodesCached` (top `Self.recentEpisodesCacheLimit`).
    /// Larger limits fall back to a one-off recompute against `state.episodes`.
    func recentEpisodes(limit: Int = 30) -> [Episode] {
        recentEpisodesView(limit: limit)
    }

    /// All episodes across every podcast, sorted newest-first.
    /// Sorting is paid once when the Rust library projection changes.
    var allEpisodesSorted: [Episode] {
        let episodes = state.episodes
        guard allEpisodeIndexesNewestFirst.count == episodes.count else {
            return episodes.sorted { $0.pubDate > $1.pubDate }
        }
        return allEpisodeIndexesNewestFirst.compactMap {
            episodes.indices.contains($0) ? episodes[$0] : nil
        }
    }

}
