import Foundation

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
        let discoveredAt = notificationDiscoveredAt ?? Date()
        let inputs = newlyInserted.compactMap { id -> FeedDiscoveryPayload.EpisodeInput? in
            guard let episode = updated.first(where: { $0.id == id }) else { return nil }
            return .init(
                episodeID: id,
                inputVersion: DesiredStatePlanner.audioVersion(episode),
                pubDate: episode.pubDate,
                title: episode.title
            )
        }.sorted { $0.episodeID.uuidString < $1.episodeID.uuidString }
        let batchVersion = ArtifactRepository.version(parts: inputs.flatMap {
            [$0.episodeID.uuidString, $0.inputVersion]
        })
        let occurrence = "discovery:\(podcastID.uuidString):\(batchVersion)"
        let policy = evaluateAutoDownload ? effectiveAutoDownload(forPodcast: podcastID) : nil
        let discoveryPayload = FeedDiscoveryPayload(
            podcastID: podcastID,
            occurrenceID: occurrence,
            discoveredAt: discoveredAt,
            episodes: inputs,
            autoDownloadPolicy: policy,
            notificationsEnabled: notificationDiscoveredAt != nil,
            policyVersion: "feed-policy-v1"
        )
        let recordsDiscovery = evaluateAutoDownload || notificationDiscoveredAt != nil
        let occurrenceJobs = inputs.isEmpty || !recordsDiscovery ? [] : [DesiredJob(
            idempotencyKey: occurrence,
            kind: .feedDiscovery,
            subjectID: podcastID,
            inputVersion: batchVersion,
            occurrenceID: occurrence,
            payload: try? Self.workflowEncoder.encode(discoveryPayload),
            priority: 40,
            resourceClass: .planning,
            maxAttempts: 8
        )]
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

    /// Marks the episode as fully played (sets `played = true`, zeroes the
    /// position so a re-play starts from the top).
    ///
    /// **Flushes the position cache before mutating.** Without the flush,
    /// a cached non-zero position for `id` would still be in
    /// `positionCache`; clearing the cache *after* the played-true write
    /// is fine, but if the app crashed between the flush and the
    /// played=true save, the user would lose both the played flag *and*
    /// the actual end-position. Flushing first means the worst case is
    /// "played=false but position correct" — recoverable next time the
    /// user opens the episode.
    func markEpisodePlayed(_ id: UUID) {
        flushPendingPositions()
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        let wasDownloaded: Bool
        if case .downloaded = state.episodes[idx].downloadState { wasDownloaded = true }
        else { wasDownloaded = false }
        var episodes = state.episodes
        episodes[idx].played = true
        episodes[idx].playbackPosition = 0
        // The cache entry for this episode (if any) is now stale — we
        // just persisted position=0 deliberately. Drop it so the next
        // tick (e.g. a stray engine observer firing post-end) doesn't
        // resurrect a non-zero position on its first eager save.
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            positionCache.removeValue(forKey: id)
            // Cached unplayed counts + in-progress feed must drop this episode.
            invalidateEpisodeProjections()
        }
        // Honour the user's "Delete after played" setting. Runs after the
        // mutation batch so the played=true write is on disk before the
        // download service flips downloadState back to .notDownloaded.
        if wasDownloaded, state.settings.autoDeleteDownloadsAfterPlayed {
            EpisodeDownloadService.shared.delete(episodeID: id)
        }
    }

    /// Clears the playback position so the episode drops out of the "Continue
    /// Listening" list without marking it played. The episode stays in the
    /// library and can be started fresh from the show detail page.
    func resetEpisodeProgress(_ id: UUID) {
        flushPendingPositions()
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[idx].playbackPosition = 0
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            positionCache.removeValue(forKey: id)
            invalidateEpisodeProjections()
        }
    }

    /// Reverts an accidental "mark played".
    func markEpisodeUnplayed(_ id: UUID) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[idx].played = false
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            // Cached unplayed counts + recent feed must re-include this episode.
            invalidateEpisodeProjections()
        }
    }

    /// Flips the user-set "starred" flag for an episode.
    func toggleEpisodeStarred(_ id: UUID) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[idx].isStarred.toggle()
        performMutationBatch {
            mutateState { $0.episodes = episodes }
        }
    }

    /// Sets the user-set "starred" flag explicitly.
    func setEpisodeStarred(_ id: UUID, _ starred: Bool) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        guard state.episodes[idx].isStarred != starred else { return }
        var episodes = state.episodes
        episodes[idx].isStarred = starred
        performMutationBatch {
            mutateState { $0.episodes = episodes }
        }
    }

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

    /// Upserts a single episode attached to a known podcast. Used by the
    /// agent's `play_external_episode` path. Re-entrant: replaying the
    /// same audio URL under the same podcast returns the existing record
    /// with its persisted `playbackPosition` intact. `imageURL` and
    /// `duration` are refreshed when they change.
    ///
    /// The caller is responsible for ensuring `podcastID` references an
    /// existing `Podcast` row (use `upsertPodcast` or `Podcast.unknownID`
    /// when no feed metadata is available).
    @discardableResult
    func upsertEpisode(
        podcastID: UUID,
        audioURL: URL,
        title: String,
        imageURL: URL?,
        duration: TimeInterval?
    ) -> Episode {
        let guid = audioURL.absoluteString
        if let idx = state.episodes.firstIndex(where: {
            $0.podcastID == podcastID && $0.guid == guid
        }) {
            var updated = state.episodes[idx]
            var changed = false
            if let imageURL, updated.imageURL != imageURL { updated.imageURL = imageURL; changed = true }
            if let duration, updated.duration != duration { updated.duration = duration; changed = true }
            if changed { mutateState { $0.episodes[idx] = updated } }
            return state.episodes[idx]
        }
        let episode = Episode(
            podcastID: podcastID,
            guid: guid,
            title: title,
            pubDate: Date(),
            duration: duration,
            enclosureURL: audioURL,
            imageURL: imageURL
        )
        performMutationBatch {
            mutateState { $0.episodes.append(episode) }
            invalidateEpisodeProjections()
        }
        WorkflowRuntime.shared.wake()
        return episode
    }

    /// Records the most-recently-loaded episode so the mini-player can be
    /// restored after an app restart. No-op when the value is unchanged.
    func setLastPlayedEpisode(_ id: UUID) {
        guard state.lastPlayedEpisodeID != id else { return }
        mutateState { $0.lastPlayedEpisodeID = id }
    }

    /// Applies the stable projection of a verified chapter artifact. No-op
    /// when an empty result would overwrite real chapter data.
    func setEpisodeChapters(_ id: UUID, chapters: [Episode.Chapter]) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        if chapters.isEmpty, let existing = state.episodes[idx].chapters, !existing.isEmpty {
            return
        }
        let projected = chapters.isEmpty ? nil : chapters
        guard state.episodes[idx].chapters != projected else { return }
        var episodes = state.episodes
        episodes[idx].chapters = projected
        mutateState { $0.episodes = episodes }
    }
}

private extension AppStateStore {
    static let workflowEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}
