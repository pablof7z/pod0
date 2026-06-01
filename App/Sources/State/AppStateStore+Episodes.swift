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
        guard var found = self.episodes.first(where: { $0.id == id }) else { return nil }
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
    /// Larger limits fall back to a one-off recompute against `self.episodes`.
    func recentEpisodes(limit: Int = 30) -> [Episode] {
        recentEpisodesView(limit: limit)
    }

    /// All episodes across every podcast, sorted newest-first.
    /// Used by the Library "All Episodes" view. Not cached — call sites should
    /// slice via `prefix(_:)` or paginate to avoid materialising the full array
    /// on every render.
    var allEpisodesSorted: [Episode] {
        self.episodes.sorted { $0.pubDate > $1.pubDate }
    }

    // MARK: - Writes

    /// Inserts episodes for an agent-synthesized podcast — the agent TTS /
    /// YouTube-ingested episodes published onto the `Podcast(kind: .synthetic)`
    /// "Agent Generated" show, and the back-catalog an agent external-play
    /// `ensurePodcast` lands for a freshly-captured feed. Returns the ids of
    /// the rows that were newly inserted (skipping any whose `guid` already
    /// exists under the same podcast, so a re-entrant publish is idempotent).
    ///
    /// This is an INSERT seam, not a merge. The legacy RSS feed-refresh merge
    /// policy — guid-match with user-mutable-state preservation
    /// (`playbackPosition` / `played` / `isStarred` / `downloadState` /
    /// `transcriptState` / triage / adSegments) — was deleted: RSS feeds are
    /// ingested by the Rust kernel (`kernelSubscribe` / `kernelRefresh`) and
    /// every preserved field now round-trips through `applyKernelState` →
    /// `EpisodeSummary.toEpisode` (M4 / D7), so the Swift preservation merge
    /// was dead duplication. Production callers here only ever pass brand-new
    /// episodes (a fresh UUID/guid per publish, or the first-ensure backlog of
    /// a new podcast), so no merge branch was reachable.
    ///
    /// NOTE: like the synthetic podcast rows themselves, these episodes live
    /// only in Swift `state`; the kernel has no model for them, so a
    /// projection tick can clobber them — a pre-existing gap tracked in
    /// `docs/BACKLOG.md` (`m9-tts-persistence`).
    @discardableResult
    func upsertEpisodes(
        _ incoming: [Episode],
        forPodcast podcastID: UUID
    ) -> [UUID] {
        guard !incoming.isEmpty else { return [] }
        var updated = self.episodes
        var existingGUIDs = Set(
            updated.lazy
                .filter { $0.podcastID == podcastID }
                .map(\.guid)
        )
        var newlyInserted: [UUID] = []
        for episode in incoming where existingGUIDs.insert(episode.guid).inserted {
            updated.append(episode)
            newlyInserted.append(episode.id)
        }
        guard !newlyInserted.isEmpty else { return [] }
        performMutationBatch {
            self.episodes = updated
            invalidateEpisodeProjections()
        }
        // Metadata-index the newly-inserted episodes for similarity search —
        // the agent-synthesized back-catalog needs title/description coverage
        // exactly like a feed's would.
        EpisodeMetadataIndexer.shared.indexNewlyInserted(
            newlyInserted,
            appStore: self
        )
        return newlyInserted
    }

    // `setEpisodePlaybackPosition(_:position:)` is implemented in
    // `AppStateStore+PositionDebounce.swift`. It writes through an in-memory
    // cache and only mutates `self.episodes` (firing the expensive save) on
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
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        let wasDownloaded: Bool
        if case .downloaded = self.episodes[idx].downloadState { wasDownloaded = true }
        else { wasDownloaded = false }
        var episodes = self.episodes
        episodes[idx].played = true
        episodes[idx].playbackPosition = 0
        // The cache entry for this episode (if any) is now stale — we
        // just persisted position=0 deliberately. Drop it so the next
        // tick (e.g. a stray engine observer firing post-end) doesn't
        // resurrect a non-zero position on its first eager save.
        performMutationBatch {
            self.episodes = episodes
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
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        guard case .downloaded = self.episodes[idx].downloadState else { return }
        kernelDeleteDownload(id)
    }

    /// Clears the playback position so the episode drops out of the "Continue
    /// Listening" list without marking it played. The episode stays in the
    /// library and can be started fresh from the show detail page.
    func resetEpisodeProgress(_ id: UUID) {
        kernelResetEpisodeProgress(id)
        flushPendingPositions()
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = self.episodes
        episodes[idx].playbackPosition = 0
        performMutationBatch {
            self.episodes = episodes
            positionCache.removeValue(forKey: id)
            invalidateEpisodeProjections()
        }
    }

    /// Reverts an accidental "mark played".
    func markEpisodeUnplayed(_ id: UUID) {
        kernelMarkUnplayed(id)
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = self.episodes
        episodes[idx].played = false
        performMutationBatch {
            self.episodes = episodes
            // Cached unplayed counts + recent feed must re-include this episode.
            invalidateEpisodeProjections()
        }
    }

    /// Flips the user-set "starred" flag for an episode.
    func toggleEpisodeStarred(_ id: UUID) {
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        let current = self.episodes[idx].isStarred
        kernelToggleStar(id, currentlyStarred: current)
        var episodes = self.episodes
        episodes[idx].isStarred.toggle()
        performMutationBatch {
            self.episodes = episodes
        }
    }

    /// Sets the user-set "starred" flag explicitly.
    func setEpisodeStarred(_ id: UUID, _ starred: Bool) {
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        guard self.episodes[idx].isStarred != starred else { return }
        kernelToggleStar(id, currentlyStarred: !starred)
        var episodes = self.episodes
        episodes[idx].isStarred = starred
        performMutationBatch {
            self.episodes = episodes
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
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = self.episodes
        episodes[idx].downloadState = newState
        performMutationBatch {
            self.episodes = episodes
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
        var episodes = self.episodes
        var changed = false
        var newlyIndexed: [UUID] = []
        for idx in episodes.indices where target.contains(episodes[idx].id) && !episodes[idx].metadataIndexed {
            episodes[idx].metadataIndexed = true
            newlyIndexed.append(episodes[idx].id)
            changed = true
        }
        guard changed else { return }
        performMutationBatch {
            self.episodes = episodes
        }
        // M4 / D7: report coverage to Rust so the flag survives a feed refresh
        // via the projection (replaces the deleted preserved-state merge).
        // Batched: one dispatch for the whole pass.
        kernelMarkEpisodesMetadataIndexed(newlyIndexed)
    }

    /// Updates the episode's transcript ingestion lifecycle.
    func setEpisodeTranscriptState(_ id: UUID, state newState: TranscriptState) {
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        let priorState = self.episodes[idx].transcriptState
        var episodes = self.episodes
        episodes[idx].transcriptState = newState
        performMutationBatch {
            self.episodes = episodes
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
        if let idx = self.episodes.firstIndex(where: {
            $0.podcastID == podcastID && $0.guid == guid
        }) {
            var updated = self.episodes[idx]
            var changed = false
            if let imageURL, updated.imageURL != imageURL { updated.imageURL = imageURL; changed = true }
            if let duration, updated.duration != duration { updated.duration = duration; changed = true }
            if changed { self.episodes[idx] = updated }
            return self.episodes[idx]
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
            self.episodes.append(episode)
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
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        if chapters.isEmpty, let existing = self.episodes[idx].chapters, !existing.isEmpty {
            return
        }
        var episodes = self.episodes
        episodes[idx].chapters = chapters.isEmpty ? nil : chapters
        self.episodes = episodes
    }
}
