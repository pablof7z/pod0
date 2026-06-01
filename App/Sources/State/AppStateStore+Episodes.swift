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
    @discardableResult
    func upsertEpisodes(
        _ incoming: [Episode],
        forPodcast podcastID: UUID
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
                merged.downloadState = prior.downloadState
                merged.transcriptState = prior.transcriptState
                // Preserve the AI Inbox triage verdict across feed refreshes;
                // without this, an archived episode reappears on the next
                // refresh and the LLM redoes the classification.
                merged.triageDecision = prior.triageDecision
                merged.triageRationale = prior.triageRationale
                merged.triageIsHero = prior.triageIsHero
                merged.adSegments = prior.adSegments
                updated[idx] = merged
            } else {
                updated.append(episode)
                newlyInserted.append(episode.id)
            }
        }
        performMutationBatch {
            state.episodes = updated
            // The didSet fingerprint catches count changes but misses pure
            // merges where count stays equal; explicit invalidation covers both.
            invalidateEpisodeProjections()
        }
        // Metadata-index every newly-inserted episode, regardless of the
        // auto-download gate — initial-subscribe paths pass false but the
        // back-catalog they introduce is exactly the population that needs
        // title/description coverage for similarity search.
        if !newlyInserted.isEmpty {
            EpisodeMetadataIndexer.shared.indexNewlyInserted(
                newlyInserted,
                appStore: self
            )
        }
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
        kernelMarkPlayed(id)
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
            state.episodes = episodes
            positionCache.removeValue(forKey: id)
            // Cached unplayed counts + in-progress feed must drop this episode.
            invalidateEpisodeProjections()
        }
        // Delete-after-played stays on the Swift side deliberately. The Rust
        // kernel owns the *mark-played* decision (`kernelMarkPlayed` above →
        // `mark_episode_played`, which only flips `played` + persists) and the
        // delete *operation* (`delete_download` → `clear_local_path`), but it
        // owns no *policy* that triggers the delete on played:
        // `auto_delete_downloads_after_played` has only two Rust consumers — the
        // settings setter and the snapshot projection — and neither
        // `mark_episode_played` nor the `ItemEnd` audio-report handler reads it.
        // Removing this gate (with the "no Rust changes" constraint) would
        // silently kill the feature. This is also the right choke point: a
        // *manual* mark-played on a downloaded episode should delete too, and
        // every mark-played path (manual + both end-of-episode callbacks)
        // converges here. Tracked for kernel-side migration in docs/BACKLOG.md
        // (delete-after-played-kernel-policy). Runs after the mutation batch so
        // the played=true write is on disk before the kernel processes the delete.
        if wasDownloaded, state.settings.autoDeleteDownloadsAfterPlayed {
            kernelDeleteDownload(id)
        }
    }

    /// Applies the Swift-owned "Delete after played" policy for an episode
    /// that the *kernel* has just marked played at end-of-item.
    ///
    /// The natural-end audio callback (`onItemEnd`) lets the Rust kernel own the
    /// mark-played-at-end decision (the `itemEnd` report drives Rust's
    /// `apply_writeback`), so it does NOT call `markEpisodePlayed`. But the
    /// kernel owns no delete-after-played *policy* (see `markEpisodePlayed`), so
    /// that callback routes here to honour the user's setting. Deletes only when
    /// the episode is currently downloaded and the setting is on; the
    /// `kernelDeleteDownload` dispatch is a no-op for a non-downloaded episode.
    func deleteDownloadIfAutoDeleteAfterPlayed(_ id: UUID) {
        guard state.settings.autoDeleteDownloadsAfterPlayed else { return }
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        guard case .downloaded = state.episodes[idx].downloadState else { return }
        kernelDeleteDownload(id)
    }

    /// Clears the playback position so the episode drops out of the "Continue
    /// Listening" list without marking it played. The episode stays in the
    /// library and can be started fresh from the show detail page.
    func resetEpisodeProgress(_ id: UUID) {
        kernelResetEpisodeProgress(id)
        flushPendingPositions()
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[idx].playbackPosition = 0
        performMutationBatch {
            state.episodes = episodes
            positionCache.removeValue(forKey: id)
            invalidateEpisodeProjections()
        }
    }

    /// Reverts an accidental "mark played".
    func markEpisodeUnplayed(_ id: UUID) {
        kernelMarkUnplayed(id)
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[idx].played = false
        performMutationBatch {
            state.episodes = episodes
            // Cached unplayed counts + recent feed must re-include this episode.
            invalidateEpisodeProjections()
        }
    }

    /// Flips the user-set "starred" flag for an episode.
    func toggleEpisodeStarred(_ id: UUID) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        let current = state.episodes[idx].isStarred
        kernelToggleStar(id, currentlyStarred: current)
        var episodes = state.episodes
        episodes[idx].isStarred.toggle()
        performMutationBatch {
            state.episodes = episodes
        }
    }

    /// Sets the user-set "starred" flag explicitly.
    func setEpisodeStarred(_ id: UUID, _ starred: Bool) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        guard state.episodes[idx].isStarred != starred else { return }
        kernelToggleStar(id, currentlyStarred: !starred)
        var episodes = state.episodes
        episodes[idx].isStarred = starred
        performMutationBatch {
            state.episodes = episodes
        }
    }

    /// Updates the episode's local download lifecycle (queued / downloading /
    /// downloaded / failed). The audio engine reads `downloaded` to decide
    /// between streaming and local file URLs.
    ///
    /// Retained (not deleted as a post-Rust-command mirror): the sole remaining
    /// caller is the Downloads Manager `.clearFailed` action, an optimistic
    /// local dismissal of a `.failed` row that has no kernel round-trip — the
    /// failed lifecycle state is iOS-side, so there is no Rust projection to
    /// defer to. The download *delete* path (`.delete`) already routes through
    /// `kernelDeleteDownload`, and the downloaded/queued/downloading states
    /// round-trip via the M2 downloads projection, so this method is no longer
    /// on those paths.
    func setEpisodeDownloadState(_ id: UUID, state newState: DownloadState) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[idx].downloadState = newState
        performMutationBatch {
            state.episodes = episodes
            // Cached `hasDownloadedByShow` set may now need to add or drop this subscription.
            invalidateEpisodeProjections()
        }
    }

    /// Marks every episode in `ids` as covered by the RAG metadata index.
    /// Single batched mutation so a backfill pass over the whole library
    /// only triggers one persisted save, regardless of episode count.
    func setEpisodesMetadataIndexed(_ ids: [UUID]) {
        guard !ids.isEmpty else { return }
        let target = Set(ids)
        var episodes = state.episodes
        var changed = false
        var newlyIndexed: [UUID] = []
        for idx in episodes.indices where target.contains(episodes[idx].id) && !episodes[idx].metadataIndexed {
            episodes[idx].metadataIndexed = true
            newlyIndexed.append(episodes[idx].id)
            changed = true
        }
        guard changed else { return }
        performMutationBatch {
            state.episodes = episodes
        }
        // M4 / D7: report coverage to Rust so the flag survives a feed refresh
        // via the projection (replaces the deleted preserved-state merge).
        // Batched: one dispatch for the whole pass.
        kernelMarkEpisodesMetadataIndexed(newlyIndexed)
    }

    /// Updates the episode's transcript ingestion lifecycle.
    func setEpisodeTranscriptState(_ id: UUID, state newState: TranscriptState) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        let priorState = state.episodes[idx].transcriptState
        var episodes = state.episodes
        episodes[idx].transcriptState = newState
        performMutationBatch {
            state.episodes = episodes
            // Cached `hasTranscribedByShow` set may now need to add or drop this subscription.
            invalidateEpisodeProjections()
        }
        // M4 / D7: report the transient status to Rust so it survives a feed
        // refresh via the projection (replaces the deleted preserved-state
        // merge). Dispatch only when the coarse status changed, to avoid a
        // rev-bump storm (guards a progress loop; in practice `.transcribing`
        // is set once at 0).
        //
        // `.ready` is deliberately NOT dispatched. `.ready` is owned by the
        // presence of the stored transcript in Rust, which arrives via the
        // separate `kernelTranscriptReport` call in `persistAndIndex` (fired
        // immediately after this). `toEpisode` derives `.ready` from that
        // transcript with priority over any status override. Dispatching
        // `"none"` here would synchronously pull a snapshot in which Rust has
        // neither the transcript (reported on the next line) nor an override,
        // briefly projecting `.none` and clobbering the `.ready` we just set.
        // Leaving the prior override in place is harmless: `toEpisode` reads
        // the transcript first, so a stale `"transcribing"` never surfaces
        // once the transcript lands.
        if case .ready = newState { return }
        let (status, message) = Self.transcriptStatusReport(for: newState)
        if Self.transcriptStatusReport(for: priorState).0 != status {
            kernelSetEpisodeTranscriptStatus(episodeID: id, status: status, message: message)
        }
    }

    /// Map a `TranscriptState` to the coarse `(status, message)` pair reported
    /// to Rust. `.none` clears the override (`"none"`). `.ready` is never
    /// reported (see `setEpisodeTranscriptState`) — it's derived by Rust from
    /// the stored transcript — but is mapped to `"none"` here for completeness
    /// so the prior-state comparison treats a `.ready → X` transition cleanly.
    private static func transcriptStatusReport(
        for state: TranscriptState
    ) -> (String, String?) {
        switch state {
        case .none, .ready: return ("none", nil)
        case .queued: return ("queued", nil)
        case .fetchingPublisher: return ("fetching_publisher", nil)
        case .transcribing: return ("transcribing", nil)
        case .failed(let message): return ("failed", message)
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
            if changed { state.episodes[idx] = updated }
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
            state.episodes.append(episode)
            invalidateEpisodeProjections()
        }
        // Trigger transcript ingest for the new episode. Auto-download is
        // skipped since the episode is already streaming; for podcasts the
        // user follows the next feed refresh will surface it via the normal
        // pipeline anyway.
        TranscriptIngestService.shared.evaluateAutoIngest(
            newEpisodeIDs: [episode.id]
        )
        return episode
    }

    /// Records the most-recently-loaded episode so the mini-player can be
    /// restored after an app restart. No-op when the value is unchanged.
    func setLastPlayedEpisode(_ id: UUID) {
        guard state.lastPlayedEpisodeID != id else { return }
        state.lastPlayedEpisodeID = id
    }

    /// Persist hydrated chapters for an episode. Used by
    /// `ChaptersHydrationService` after asynchronously fetching the JSON
    /// referenced by `episode.chaptersURL`. No-op when `chapters` is empty
    /// AND the episode already has chapters — we never overwrite real data
    /// with an empty result.
    func setEpisodeChapters(_ id: UUID, chapters: [Episode.Chapter]) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        if chapters.isEmpty, let existing = state.episodes[idx].chapters, !existing.isEmpty {
            return
        }
        var episodes = state.episodes
        episodes[idx].chapters = chapters.isEmpty ? nil : chapters
        state.episodes = episodes
    }
}
