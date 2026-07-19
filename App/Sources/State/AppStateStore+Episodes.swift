import Foundation

// MARK: - Episodes

extension AppStateStore {

    // MARK: - Reads

    /// Returns the live episode record matching `id`, or `nil` when not found.
    func episode(id: UUID) -> Episode? {
        state.episodes.first(where: { $0.id == id })
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
    /// Used by the Library "All Episodes" view. Not cached — call sites should
    /// slice via `prefix(_:)` or paginate to avoid materialising the full array
    /// on every render.
    var allEpisodesSorted: [Episode] {
        state.episodes.sorted { $0.pubDate > $1.pubDate }
    }

    // MARK: - Temporary native adjunct writes

    /// Updates stable local-file evidence. Active/retry/failure lifecycle is
    /// owned exclusively by JobStore.
    @discardableResult
    func setEpisodeDownloadState(
        _ id: UUID,
        state newState: DownloadState
    ) -> EpisodeTransitionResult {
        guard let episode = episode(id: id) else {
            return rejectEpisodeTransition("Episode does not exist")
        }
        switch newState {
        case .notDownloaded:
            return applyDownloadEvent(.userRemoved, episodeID: id)
        case .downloaded(let url, let byteCount):
            let selected = try? ArtifactRepository(
                fileURL: persistence.episodeStore.fileURL
            ).current(kind: .downloadFile, subjectID: id)
            let inputVersion = DesiredStatePlanner.audioVersion(episode)
            guard let selected,
                  selected.integrity == .available,
                  selected.inputVersion == inputVersion,
                  selected.location == url.path,
                  let data = try? Data(contentsOf: url, options: .mappedIfSafe),
                  selected.contentHash == ArtifactRepository.hash(data),
                  Int64(data.count) == byteCount else {
                return rejectEpisodeTransition(
                    "Downloaded state requires matching selected artifact evidence"
                )
            }
            return applyDownloadEvent(.artifactCommitted(.init(
                inputVersion: selected.inputVersion,
                contentHash: selected.contentHash, fileURL: url,
                byteCount: byteCount
            )), episodeID: id)
        }
    }

}
