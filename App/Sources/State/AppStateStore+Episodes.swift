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
        // When the kernel has this episode loaded (playing or paused), apply
        // the live kernel position as a floor. This covers the window between
        // a pause event and the next debounce flush — e.g. when the user
        // navigates back to the detail view immediately after pausing.
        if let np = kernel?.nowPlaying,
           let idStr = np.episodeId,
           let npId = UUID(uuidString: idStr),
           npId == id,
           np.positionSecs > found.playbackPosition {
            found.playbackPosition = np.positionSecs
        }
        return found
    }

    /// Episodes belonging to the given podcast, newest publish-date first.
    /// Rust owns membership, archive visibility, ordering, and caps; Swift
    /// resolves the returned ids for native rendering and legacy test callers.
    func episodes(forPodcast id: UUID) -> [Episode] {
        rustEpisodes(forPodcast: id)
    }

    /// Episodes the user has started but not finished. Rust owns the
    /// in-progress membership and ordering.
    var inProgressEpisodes: [Episode] {
        rustInProgressEpisodes()
    }

    /// Recently published, unplayed episodes across subscriptions. Rust owns
    /// membership and ordering.
    func recentEpisodes(limit: Int = 30) -> [Episode] {
        rustRecentEpisodes(limit: limit)
    }

    // MARK: - Writes

    /// Inserts episodes into the Swift render store. Production feed and agent
    /// episode ingestion now route through the Rust kernel (`kernelEnsurePodcast`
    /// / `kernelAddEpisode`) and project back through `applyKernelState`; this
    /// helper remains for focused AppStateStore tests and legacy fixtures.
    ///
    /// This is an INSERT seam, not a merge. The legacy RSS feed-refresh merge
    /// policy — guid-match with user-mutable-state preservation
    /// (`playbackPosition` / `played` / `isStarred` / `downloadState` /
    /// `transcriptState` / triage / adSegments) — was deleted: RSS feeds are
    /// ingested by the Rust kernel (`kernelSubscribe` / `kernelRefresh`) and
    /// every preserved field now round-trips through `applyKernelState` →
    /// `EpisodeSummary.toEpisode` (M4 / D7), so the Swift preservation merge
    /// was dead duplication.
    ///
    /// NOTE: like the feed-less podcast rows themselves, these episodes live
    /// only in Swift `state`; the kernel has no model for them, so a
    /// projection tick can clobber them — a pre-existing gap tracked in
    /// `docs/BACKLOG.md` (`external-feed-ensure-kernel-seed`).
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
        positionCache.removeValue(forKey: id)
        // Delete-after-played is now kernel-owned policy (D0). `kernelMarkPlayed`
        // dispatches `inbox/mark_listened`, whose Rust handler reads
        // `auto_delete_downloads_after_played` and removes the local download
        // itself. The previous Swift gate here (and the `onItemEnd`
        // reaction) have been removed so the policy lives in exactly one place.
    }

    /// Clears the playback position so the episode drops out of the "Continue
    /// Listening" list without marking it played. The episode stays in the
    /// library and can be started fresh from the show detail page.
    func resetEpisodeProgress(_ id: UUID) {
        kernelResetEpisodeProgress(id)
        flushPendingPositions()
        positionCache.removeValue(forKey: id)
    }

    /// Reverts an accidental "mark played".
    func markEpisodeUnplayed(_ id: UUID) {
        kernelMarkUnplayed(id)
    }

    /// Flips the user-set "starred" flag for an episode.
    func toggleEpisodeStarred(_ id: UUID) {
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        let current = self.episodes[idx].isStarred
        kernelToggleStar(id, currentlyStarred: current)
    }

    /// Sets the user-set "starred" flag explicitly.
    func setEpisodeStarred(_ id: UUID, _ starred: Bool) {
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        guard self.episodes[idx].isStarred != starred else { return }
        kernelToggleStar(id, currentlyStarred: !starred)
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
            // Compatibility no-op; Rust owns downloaded-show projections.
            invalidateEpisodeProjections()
        }
    }

    /// Updates the episode's transcript ingestion lifecycle.
    ///
    /// `provider` (optional) names the STT service driving the transition — the
    /// transcription pipeline passes it when moving to `.transcribing` / `.failed`
    /// so the kernel's `transcript.attempt` / `transcript.failed` Diagnostics
    /// event can say *which* service is at work. `nil` for callers that don't
    /// know or don't care (the generic UI state flips).
    func setEpisodeTranscriptState(
        _ id: UUID,
        state newState: TranscriptState,
        provider: String? = nil
    ) {
        guard let idx = self.episodes.firstIndex(where: { $0.id == id }) else { return }
        let priorState = self.episodes[idx].transcriptState
        var episodes = self.episodes
        episodes[idx].transcriptState = newState
        performMutationBatch {
            self.episodes = episodes
            // Compatibility no-op; Rust owns transcribed-show projections.
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
        guard let report = Self.transcriptStatusReport(for: newState, kernel: kernel) else { return }
        if Self.transcriptStatusReport(for: priorState, kernel: kernel)?.status != report.status {
            kernelSetEpisodeTranscriptStatus(episodeID: id, report: report, provider: provider)
        }
    }

    @discardableResult
    func kernelReportEpisodeTranscriptState(
        episodeID id: UUID,
        state: TranscriptState,
        provider: String? = nil
    ) -> DispatchResult? {
        guard let report = Self.transcriptStatusReport(for: state, kernel: kernel) else { return nil }
        return kernelSetEpisodeTranscriptStatus(episodeID: id, report: report, provider: provider)
    }

    private struct TranscriptStatusReport: Decodable {
        let status: String?
        let message: String?
        let error: String?
    }

    /// Ask Rust to map a raw `TranscriptState` tag into the coarse status
    /// override reported back to the kernel. Swift only serializes the local
    /// enum and performs the callback; Rust owns the status/message policy.
    private static func transcriptStatusReport(
        for state: TranscriptState,
        kernel: KernelModel?
    ) -> (status: String, message: String?)? {
        guard let handle = kernel?.podcastHandlePointer else { return nil }
        var request = transcriptStatePayload(for: state)
        request["op"] = "transcript_status_report"
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        let envelope = json.withCString { ptr -> String? in
            guard let result = nmp_app_podcast_agent_action_tool(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            return String(cString: result)
        }
        guard let envelope,
              let responseData = envelope.data(using: .utf8),
              let response = try? JSONDecoder().decode(TranscriptStatusReport.self, from: responseData),
              response.error == nil,
              let status = response.status,
              !status.isEmpty
        else { return nil }
        return (status, response.message)
    }

    static func transcriptStatePayload(for state: TranscriptState) -> [String: Any] {
        switch state {
        case .none: return ["state": "none"]
        case .ready: return ["state": "ready"]
        case .queued: return ["state": "queued"]
        case .fetchingPublisher: return ["state": "fetching_publisher"]
        case .transcribing: return ["state": "transcribing"]
        case .failed(let message): return ["state": "failed", "message": message]
        }
    }

    @discardableResult
    private func kernelSetEpisodeTranscriptStatus(
        episodeID id: UUID,
        report: (status: String, message: String?),
        provider: String?
    ) -> DispatchResult? {
        kernelSetEpisodeTranscriptStatus(
            episodeID: id,
            status: report.status,
            message: report.message,
            provider: provider
        )
    }

    /// Records the most-recently-loaded episode so the mini-player can be
    /// restored after an app restart. No-op when the value is unchanged.
    func setLastPlayedEpisode(_ id: UUID) {
        guard state.lastPlayedEpisodeID != id else { return }
        state.lastPlayedEpisodeID = id
    }

}
