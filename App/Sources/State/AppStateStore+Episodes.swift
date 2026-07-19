import Foundation
import Pod0Core

// MARK: - Episodes

extension AppStateStore {

    // MARK: - Reads
    //
    // Reads fold the position-debounce cache into the result so a freshly-
    // updated playhead is visible to UI surfaces (in-progress carousel,
    // resume-from-position, episode detail) without waiting for the next
    // disk flush. See `AppStateStore+PositionDebounce.swift` for the
    // cache's lifecycle.

    /// Returns the live episode record matching `id`, or `nil` when not found.
    func episode(id: UUID) -> Episode? {
        guard var found = state.episodes.first(where: { $0.id == id }) else { return nil }
        if let cached = cachedPosition(for: id) {
            found.playbackPosition = cached
        }
        return found
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

    // MARK: - Writes

    /// Inserts new episodes and updates existing ones (matched by `guid`)
    /// for the given subscription. Episodes whose `guid` already exists in
    /// the store are merged: the publisher fields refresh while the user-
    /// mutable playback state (`playbackPosition`, `played`, `downloadState`,
    /// `transcriptState`) is preserved.
    ///
    /// When `evaluateAutoDownload` is true, the episode mutation atomically
    /// records a durable discovery occurrence. Its executor later materializes
    /// bounded follow-on intent from the exact inserted batch.
    @discardableResult
    func upsertEpisodes(
        _ incoming: [Episode],
        forPodcast podcastID: UUID,
        evaluateAutoDownload: Bool = false,
        notificationDiscoveredAt: Date? = nil
    ) -> [UUID] {
        guard !incoming.isEmpty else { return [] }
        if isSharedLibraryAuthoritative,
           !isApplyingSharedLibraryProjection,
           podcast(id: podcastID)?.kind == .rss {
            return []
        }
        var updated = state.episodes
        let existingByGUID = Dictionary(
            updated.enumerated()
                .filter { $0.element.podcastID == podcastID }
                .map { ($0.element.guid, $0.offset) },
            uniquingKeysWith: { first, _ in first }
        )
        var newlyInserted: [UUID] = []
        for episode in incoming {
            if let idx = existingByGUID[episode.guid] {
                let prior = updated[idx]
                var merged = episode
                merged.id = prior.id
                merged.playbackPosition = prior.playbackPosition
                merged.played = prior.played
                merged.isStarred = prior.isStarred
                let audioInputChanged = DesiredStatePlanner.audioVersion(prior)
                    != DesiredStatePlanner.audioVersion(merged)
                merged.downloadState = audioInputChanged ? .notDownloaded : prior.downloadState
                merged.transcriptState = audioInputChanged ? .none : prior.transcriptState
                merged.requestedTranscriptProvider = prior.requestedTranscriptProvider
                // Preserve AI-compiled/hydrated chapters when the incoming RSS episode
                // doesn't supply new ones; RSS never carries ad segments so always keep.
                if merged.chapters == nil || merged.chapters!.isEmpty {
                    merged.chapters = prior.chapters
                }
                merged.adSegments = prior.adSegments
                updated[idx] = merged
            } else {
                updated.append(episode)
                newlyInserted.append(episode.id)
            }
        }
        let occurrenceJobs = feedDiscoveryJobs(
            podcastID: podcastID,
            episodeIDs: newlyInserted,
            episodes: updated,
            evaluateAutoDownload: evaluateAutoDownload,
            notificationDiscoveredAt: notificationDiscoveredAt
        )
        performMutationBatch {
            mutateState(ensuring: occurrenceJobs) { $0.episodes = updated }
            // The didSet fingerprint catches count changes but misses pure
            // merges where count stays equal; explicit invalidation covers both.
            invalidateEpisodeProjections()
        }
        WorkflowRuntime.shared.wake()
        return newlyInserted
    }

    // `setEpisodePlaybackPosition(_:position:)` is implemented in
    // `AppStateStore+PositionDebounce.swift`. It writes through an in-memory
    // cache and only mutates `state.episodes` (firing the expensive save) on
    // an eager-first / 5-second-trailing / 30-second-cap schedule. This is
    // the file's single highest-frequency caller; routing it through the
    // cache is the entire point of that companion file.

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

    /// Updates the episode's transcript ingestion lifecycle.
    @discardableResult
    func setEpisodeTranscriptState(
        _ id: UUID,
        state newState: TranscriptState
    ) -> EpisodeTransitionResult {
        guard let episode = episode(id: id) else {
            return rejectEpisodeTransition("Episode does not exist")
        }
        let inputVersion = DesiredStatePlanner.audioVersion(episode)
        switch newState {
        case .none:
            return applyTranscriptEvent(
                .artifactInvalidated(inputVersion: inputVersion), episodeID: id
            )
        case .ready(let source):
            let selected = try? ArtifactRepository(
                fileURL: persistence.episodeStore.fileURL
            ).current(kind: .transcript, subjectID: id)
            guard let selected,
                  selected.integrity == .available,
                  selected.inputVersion == inputVersion,
                  let location = selected.location else {
                return rejectEpisodeTransition(
                    "Transcript state requires current selected artifact evidence"
                )
            }
            let url = URL(fileURLWithPath: location)
            guard let data = TranscriptStore.shared.verifiedData(at: url, episodeID: id),
                  ArtifactRepository.hash(data) == selected.contentHash else {
                return rejectEpisodeTransition("Transcript artifact is not verified")
            }
            return applyTranscriptEvent(.artifactCommitted(.init(
                inputVersion: selected.inputVersion,
                contentHash: selected.contentHash,
                fileURL: url, source: source
            )), episodeID: id)
        }
    }

}
