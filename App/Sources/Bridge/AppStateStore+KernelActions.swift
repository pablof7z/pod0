import Foundation

// MARK: - Kernel-backed mutation entry points
//
// All domain mutations route through these methods. Each delegates to
// `kernel.dispatch`, which (1) synchronously enqueues the action in Rust,
// (2) calls `pullPodcastSnapshotIfChanged` immediately, and (3) triggers the
// `withObservationTracking` listener in `attachKernel` so `AppState` updates
// before the next frame.
//
// Namespaces (verified against apps/nmp-app-podcast/src/ffi/actions/):
//   "podcast"          – subscribe, unsubscribe, refresh/refresh_all,
//                        download, delete_download, star_episode
//   "podcast.inbox"    – mark_listened
//   "podcast.player"   – cancel_download

extension AppStateStore {

    // MARK: - Subscription / library

    /// Subscribe to a feed URL. Dispatches to Rust and waits (up to
    /// `timeout`) for the new podcast to appear in the projected state.
    /// Preserves the `throws Podcast` signature that `AddShowSheet`,
    /// `DiscoverSearchForm`, and `OPMLImportSheet` depend on.
    @discardableResult
    func kernelSubscribe(feedURL: String,
                         timeout: Duration = .seconds(30)) async throws -> Podcast {
        guard let kern = kernel else {
            throw SubscriptionService.AddError.transport("Kernel not available")
        }
        let trimmed = feedURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let url = URL(string: trimmed) else {
            throw SubscriptionService.AddError.invalidURL
        }
        if let existing = podcast(feedURL: url),
           subscription(podcastID: existing.id) != nil {
            throw SubscriptionService.AddError.alreadySubscribed(title: existing.title)
        }
        kern.dispatch(namespace: "podcast", body: ["op": "subscribe", "feed_url": trimmed])
        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline {
            if let p = podcast(feedURL: url),
               subscription(podcastID: p.id) != nil { return p }
            try await Task.sleep(for: .milliseconds(300))
        }
        throw SubscriptionService.AddError.transport(
            "Feed did not appear in library after \(timeout). It may still arrive.")
    }

    /// Unsubscribe from a podcast and remove it from the library.
    func kernelUnsubscribe(podcastID: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "unsubscribe", "podcast_id": podcastID.uuidString])
    }

    /// Trigger a full feed refresh for every subscription.
    func kernelRefreshAll() {
        kernel?.dispatch(namespace: "podcast", body: ["op": "refresh_all"])
    }

    /// Refresh a single podcast feed.
    func kernelRefresh(podcastID: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "refresh", "podcast_id": podcastID.uuidString])
    }

    /// Dispatch a NIP-F4 (`kind:10154`) Nostr podcast discovery sweep
    /// (namespace: podcast). Rust queries the configured relay (with an HTTP
    /// gateway fallback) and surfaces results on
    /// `podcastSnapshot.nostrResults` via the reactive push seam — no spinner,
    /// no local loading state. Results appear as the relay responds.
    ///
    /// `relayURL` overrides the default relay: a `wss://`/`ws://` URL is used
    /// as the relay socket, an `http(s)://` URL as the gateway. `nil` uses the
    /// Claim a kind:10154 NIP-F4 discovery subscription. NMP kernel routes
    /// through app relays + the user's NIP-65 outbox read relays automatically.
    func kernelDiscoverNostrClaim() {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "discover_nostr", "consumer_id": "nostr-discover-view"])
    }

    /// Release the kind:10154 discovery subscription claimed by this view.
    func kernelDiscoverNostrRelease() {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "discover_nostr", "consumer_id": "nostr-discover-view", "release": true])
    }

    // MARK: - Playback dispatch (M1 Part 3)

    /// Load an episode into the Rust actor without starting playback.
    /// Rust resolves the URL and position, stages the actor, and dispatches
    /// `AudioCommand::Load` to iOS. iOS routes the command to `AudioEngine`.
    func kernelLoad(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "load", "episode_id": episodeID.uuidString])
    }

    /// Resume playback of the currently-staged episode. Dispatches
    /// `AudioCommand::Play` only — no reload, no position reset.
    func kernelResume() {
        kernel?.dispatch(namespace: "podcast.player", body: ["op": "resume"])
    }

    /// Pause playback.
    func kernelPause() {
        kernel?.dispatch(namespace: "podcast.player", body: ["op": "pause"])
    }

    /// Seek to `positionSecs`.
    func kernelSeek(positionSecs: Double) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "seek", "position_secs": positionSecs])
    }

    /// Write `positionSecs` for `episodeID` directly to the store without
    /// dispatching an audio command. Use for paused seeks where the engine
    /// has already moved but no `Playing` reports are in flight — this keeps
    /// Rust's saved position in sync so the next `kernelLoad` returns the
    /// correct resume point instead of snapping back to a stale position.
    func kernelPersistPosition(episodeID: UUID, positionSecs: Double) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "persist_position",
                                "episode_id": episodeID.uuidString,
                                "position_secs": positionSecs])
    }

    /// Play an episode from its saved position (or beginning).
    /// Rust stages the actor and dispatches `AudioCommand::Load + Play`.
    func kernelPlay(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "play", "episode_id": episodeID.uuidString])
    }

    // MARK: - Episode state

    /// Mark an episode as fully played (namespace: podcast.inbox).
    func kernelMarkPlayed(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast.inbox",
                         body: ["op": "mark_listened", "episode_id": id.uuidString])
    }

    /// Revert an accidental mark-played (namespace: podcast.inbox).
    func kernelMarkUnplayed(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast.inbox",
                         body: ["op": "mark_unlistened", "episode_id": id.uuidString])
    }

    /// Reset the playback position to zero without marking the episode played (namespace: podcast.player).
    func kernelResetEpisodeProgress(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "reset_progress", "episode_id": id.uuidString])
    }

    /// Toggle the starred flag for an episode (namespace: podcast).
    /// Pass the current starred state so Rust sets it explicitly rather than
    /// toggling from potentially-stale kernel state.
    func kernelToggleStar(_ id: UUID, currentlyStarred: Bool) {
        kernel?.dispatch(namespace: "podcast",
                         body: [
                             "op": "star_episode",
                             "episode_id": id.uuidString,
                             "starred": !currentlyStarred,
                         ])
    }

    // MARK: - Queue (podcast.queue namespace)

    /// Push an episode to the back of the Rust-owned Up Next queue.
    func kernelEnqueueLast(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.queue",
                         body: ["op": "add_last", "episode_id": episodeID.uuidString])
    }

    /// Push an episode to the front of the Rust-owned Up Next queue (Play Next).
    func kernelEnqueueNext(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.queue",
                         body: ["op": "add_next", "episode_id": episodeID.uuidString])
    }

    /// Remove all occurrences of an episode from the Rust-owned Up Next queue.
    func kernelDequeueEpisode(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.queue",
                         body: ["op": "remove", "episode_id": episodeID.uuidString])
    }

    /// Empty the Rust-owned Up Next queue.
    func kernelClearQueue() {
        kernel?.dispatch(namespace: "podcast.queue", body: ["op": "clear"])
    }

    // MARK: - Chapters

    /// Fetch Podcasting 2.0 chapters JSON for an episode (namespace: podcast).
    /// Rust fetches the `chaptersURL`, parses the JSON, stores the results in
    /// `PodcastStore`, increments the snapshot rev, and the next projection
    /// tick maps them onto `Episode.chapters` via `applyKernelState`.
    func kernelFetchChapters(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "fetch_chapters", "episode_id": episodeID.uuidString])
    }

    // MARK: - Ad segments

    /// Persist detected ad-break intervals for an episode (namespace: podcast.player).
    /// Rust stores them in `PodcastStore` and (if the episode is currently loaded)
    /// pushes them into the player actor so auto-skip fires on the next tick.
    func kernelSetAdSegments(episodeID: UUID, segments: [Episode.AdSegment]) {
        let segDicts: [[String: Any]] = segments.map { seg in
            ["id": seg.id.uuidString, "start_secs": seg.start, "end_secs": seg.end, "kind": seg.kind.rawValue]
        }
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "set_ad_segments", "episode_id": episodeID.uuidString, "segments": segDicts])
    }

    // MARK: - Subscription settings

    /// Update the auto-download policy for a single podcast (namespace: podcast).
    /// Rust receives both `enabled` and `wifi_only` so the cellular-allowed
    /// override is stored. iOS `.latestN` and `.allNew` both map to
    /// `enabled: true` since the Rust store records only on/off.
    func kernelSetAutoDownload(podcastID: UUID, policy: AutoDownloadPolicy) {
        kernel?.dispatch(namespace: "podcast",
                         body: [
                             "op": "set_auto_download",
                             "podcast_id": podcastID.uuidString,
                             "enabled": policy.mode != .off,
                             "wifi_only": policy.wifiOnly
                         ])
    }

    // MARK: - Downloads

    /// Queue a download (namespace: podcast).
    func kernelDownload(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "download", "episode_id": id.uuidString])
    }

    /// Cancel an in-progress or queued download (namespace: podcast.player).
    func kernelCancelDownload(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "cancel_download", "episode_id": id.uuidString])
    }

    /// Delete a downloaded episode file (namespace: podcast).
    func kernelDeleteDownload(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "delete_download", "episode_id": id.uuidString])
    }

    // MARK: - Transcripts

    /// Report a completed transcript to the Rust kernel (M5.2).
    /// Delegates to `KernelModel.sendTranscriptReport` which has access to
    /// the raw `podcastHandle` pointer.
    func kernelTranscriptReport(episodeID: UUID, text: String) {
        kernel?.sendTranscriptReport(episodeID: episodeID, text: text)
    }

    // MARK: - M4 capability reports (D7)

    /// One row of a triage batch dispatched to the Rust kernel.
    struct KernelTriagePatch {
        let episodeID: UUID
        /// Raw `TriageDecision` rawValue, or `"none"` to clear.
        let decision: String
        let isHero: Bool
        let rationale: String?
    }

    /// Report a batch of AI Inbox triage decisions to the Rust kernel so they
    /// survive a feed refresh via the projection (replaces the deleted
    /// preserved-state merge). Batched — one dispatch (one rev bump + one
    /// library re-encode) per `applyTriageDecisions` pass rather than one per
    /// episode. `decision` is the raw `TriageDecision` rawValue, or `"none"`
    /// to clear a prior decision (the `clearTriageDecision` path).
    func kernelSetEpisodeTriage(_ patches: [KernelTriagePatch]) {
        guard !patches.isEmpty else { return }
        let decisions: [[String: Any]] = patches.map { patch in
            var row: [String: Any] = [
                "episode_id": patch.episodeID.uuidString,
                "decision": patch.decision,
                "is_hero": patch.isHero,
            ]
            if let rationale = patch.rationale { row["rationale"] = rationale }
            return row
        }
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "set_episode_triage", "decisions": decisions])
    }

    /// Report a batch of RAG-metadata-indexed episodes to the Rust kernel.
    /// Batched so a whole backfill pass costs one dispatch (one rev bump +
    /// one library re-encode) rather than one per episode.
    func kernelMarkEpisodesMetadataIndexed(_ ids: [UUID]) {
        guard !ids.isEmpty else { return }
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "mark_episodes_metadata_indexed",
                                "episode_ids": ids.map(\.uuidString)])
    }

    /// Report the transient transcript-ingestion status for an episode. Rust
    /// derives `.ready` from the stored transcript; iOS reports the in-progress
    /// / failed / cleared states here. `status` is `"queued"` |
    /// `"fetching_publisher"` | `"transcribing"` | `"failed"` | `"none"`
    /// (clear). `message` carries the user-facing error for `"failed"`.
    func kernelSetEpisodeTranscriptStatus(
        episodeID: UUID,
        status: String,
        message: String?
    ) {
        var body: [String: Any] = [
            "op": "set_episode_transcript_status",
            "episode_id": episodeID.uuidString,
            "status": status,
        ]
        if let message { body["message"] = message }
        kernel?.dispatch(namespace: "podcast", body: body)
    }
}
