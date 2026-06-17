import Foundation
import os

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
        let trimmed = feedURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = SubscriptionService.normalizedFeedURL(from: trimmed) else {
            throw SubscriptionService.AddError.invalidURL
        }
        guard let kern = kernel else {
            throw SubscriptionService.AddError.transport("Kernel not available")
        }
        let normalizedFeedURL = url.absoluteString
        let dispatch = kern.dispatch(PodcastKernelAction.Subscribe(feedUrl: normalizedFeedURL))
        if case let .failure(message) = dispatch {
            if message.localizedCaseInsensitiveContains("already subscribed"),
               let existing = podcast(feedURL: url) {
                throw SubscriptionService.AddError.alreadySubscribed(title: existing.title)
            }
            throw SubscriptionService.AddError.transport(message)
        }
        // React to the projected library landing the followed feed instead of
        // polling on a 300ms timer. `podcast(feedURL:)` and the Rust-owned
        // subscription status projection re-fire the instant `applyKernelState`
        // writes the subscribed feed.
        if let podcast = await awaitState(timeout: timeout, body: { [weak self] () -> Podcast? in
            guard let self,
                  let p = self.podcast(feedURL: url),
                  self.rustIsAlreadySubscribed(feedURL: normalizedFeedURL, ownerPubkey: nil) else { return nil }
            return p
        }) {
            return podcast
        }
        throw SubscriptionService.AddError.transport(
            "Feed did not appear in library after \(timeout). It may still arrive.")
    }

    /// Unsubscribe from a podcast and remove it from the library.
    func kernelUnsubscribe(podcastID: UUID) {
        kernel?.dispatch(PodcastKernelAction.Unsubscribe(podcastId: podcastID.uuidString))
    }

    /// Trigger a full feed refresh for every subscription.
    func kernelRefreshAll() {
        kernel?.dispatch(PodcastKernelAction.RefreshAll())
    }

    /// Refresh a single podcast feed.
    func kernelRefresh(podcastID: UUID) {
        kernel?.dispatch(PodcastKernelAction.Refresh(podcastId: podcastID.uuidString))
    }

    /// Queue every currently eligible episode in a podcast. Rust owns the
    /// eligibility pass and queue idempotence; Swift only sends the show intent.
    func kernelDownloadPodcast(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "download_podcast", "podcast_id": id.uuidString])
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

    /// Subscribe to a feedless NIP-F4 (`kind:54`) Nostr podcast by author pubkey.
    ///
    /// Rust opens a `kind:54` relay subscription through the kernel's relay pool
    /// (D7 — no iOS WebSocket) and upserts a followed feedless show row so the
    /// podcast appears in the library immediately. Episodes arrive asynchronously
    /// via the reactive observer and ride the snapshot push seam.
    ///
    /// Dispatches `subscribe_nostr` (namespace: podcast) and waits up to
    /// `timeout` for the feedless show row to land in the projected podcasts.
    /// Returns the `Podcast` on success; throws on timeout.
    @discardableResult
    func kernelSubscribeNostr(authorPubkeyHex: String,
                              showTitle: String? = nil,
                              timeout: Duration = .seconds(10)) async throws -> Podcast {
        guard let kern = kernel else {
            throw SubscriptionService.AddError.transport("Kernel not available")
        }
        let pubkey = authorPubkeyHex.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !pubkey.isEmpty else {
            throw SubscriptionService.AddError.transport("Author pubkey is empty")
        }
        var body: [String: Any] = ["op": "subscribe_nostr", "author_pubkey_hex": pubkey]
        if let title = showTitle { body["show_title"] = title }
        kern.dispatch(namespace: "podcast", body: body)
        // Wait for the feedless show row to appear in the projected podcasts.
        // The Rust handler calls `subscribe_feedless_show`, which creates the
        // row and bumps rev; the next snapshot push frame lands the podcast.
        if let podcast = await awaitState(timeout: timeout, body: { [weak self] () -> Podcast? in
            self?.rustPodcastForOwnerPubkey(pubkey)
        }) {
            return podcast
        }
        throw SubscriptionService.AddError.transport(
            "Feedless show did not appear in library after \(timeout).")
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
    @discardableResult
    func kernelPause() -> DispatchResult? {
        kernel?.dispatch(namespace: "podcast.player", body: ["op": "pause"])
    }

    /// Arm or clear the Rust-owned sleep timer. Duration mode configures the
    /// native OS timer through a Rust `AudioCommand`; end-of-episode mode stays
    /// entirely in the Rust player actor and suppresses auto-advance on ItemEnd.
    @discardableResult
    func kernelSetSleepTimer(_ timer: PlaybackSleepTimer) -> DispatchResult? {
        var body: [String: Any] = ["op": "set_sleep_timer"]
        switch timer {
        case .off:
            body["secs"] = NSNull()
        case .minutes(let minutes):
            body["secs"] = max(1, minutes) * 60
        case .endOfEpisode:
            body["secs"] = NSNull()
            body["end_of_episode"] = true
        }
        return kernel?.dispatch(namespace: "podcast.player", body: body)
    }

    /// Set playback speed through the Rust player actor.
    @discardableResult
    func kernelSetSpeed(_ speed: Double) -> DispatchResult? {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "set_speed", "speed": speed])
    }

    /// Seek to `positionSecs`.
    @discardableResult
    func kernelSeek(positionSecs: Double) -> DispatchResult? {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "seek", "position_secs": positionSecs])
    }

    /// Skip forward by `secs` from Rust's current player position.
    @discardableResult
    func kernelSkipForward(secs: Double?) -> DispatchResult? {
        var body: [String: Any] = ["op": "skip_forward"]
        if let secs { body["secs"] = secs }
        return kernel?.dispatch(namespace: "podcast.player", body: body)
    }

    /// Skip backward by `secs` from Rust's current player position.
    @discardableResult
    func kernelSkipBackward(secs: Double?) -> DispatchResult? {
        var body: [String: Any] = ["op": "skip_backward"]
        if let secs { body["secs"] = secs }
        return kernel?.dispatch(namespace: "podcast.player", body: body)
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
    @discardableResult
    func kernelPlay(
        episodeID: UUID,
        startSeconds: Double? = nil,
        endSeconds: Double? = nil
    ) -> DispatchResult? {
        kernelPlay(
            episodeID: episodeID.uuidString,
            startSeconds: startSeconds,
            endSeconds: endSeconds
        )
    }

    /// Play a raw episode id. Rust owns episode lookup, resume position, and
    /// optional bounded-segment enforcement.
    @discardableResult
    func kernelPlay(
        episodeID: String,
        startSeconds: Double? = nil,
        endSeconds: Double? = nil
    ) -> DispatchResult? {
        var body: [String: Any] = ["op": "play", "episode_id": episodeID]
        if let startSeconds { body["start_secs"] = startSeconds }
        if let endSeconds { body["end_secs"] = endSeconds }
        return kernel?.dispatch(namespace: "podcast.player", body: body)
    }

    // MARK: - Inbox triage

    /// Ask the kernel to (re)triage the inbox (namespace: podcast.inbox).
    ///
    /// The Rust kernel owns inbox triage (M5): it selects candidates, runs
    /// the classifier, and projects per-episode decisions onto
    /// `Episode.triageDecision` every snapshot tick. Swift only displays
    /// the result. This `triage` op is the "recompute / force a visible
    /// tick" signal — fired on appear and pull-to-refresh so freshly
    /// arrived episodes get a decision without Swift running any
    /// orchestration of its own.
    func kernelTriageInbox() {
        kernel?.dispatch(namespace: "podcast.inbox", body: ["op": "triage"])
    }

    // MARK: - Episode state

    /// Mark an episode as fully played (namespace: podcast.inbox).
    @discardableResult
    func kernelMarkPlayed(_ id: UUID) -> DispatchResult? {
        kernelMarkPlayed(episodeID: id.uuidString)
    }

    /// Mark an episode as fully played by raw id. Rust owns id validation.
    @discardableResult
    func kernelMarkPlayed(episodeID: String) -> DispatchResult? {
        kernel?.dispatch(namespace: "podcast.inbox",
                         body: ["op": "mark_listened", "episode_id": episodeID])
    }

    /// Revert an accidental mark-played (namespace: podcast.inbox).
    @discardableResult
    func kernelMarkUnplayed(_ id: UUID) -> DispatchResult? {
        kernelMarkUnplayed(episodeID: id.uuidString)
    }

    /// Revert an accidental mark-played by raw id. Rust owns id validation.
    @discardableResult
    func kernelMarkUnplayed(episodeID: String) -> DispatchResult? {
        kernel?.dispatch(namespace: "podcast.inbox",
                         body: ["op": "mark_unlistened", "episode_id": episodeID])
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

    /// Generate (or fetch a cached) AI summary for an episode via the Rust
    /// kernel LLM pipeline (namespace: podcast). Replaces the deleted Swift
    /// `LiveEpisodeSummarizerAdapter`.
    ///
    /// If a summary is already stamped on the projected episode, returns it
    /// immediately (the prompt is fixed, so a cached value is authoritative).
    /// Otherwise dispatches `summarize_episode` — a fire-and-forget action whose
    /// result arrives asynchronously on the snapshot projection — and waits, up
    /// to `timeout`, for `episode.summary` to populate (mirroring
    /// `kernelSubscribe`'s dispatch-then-await-projection pattern). Returns
    /// `nil` on timeout (e.g. Ollama offline); the caller falls back to the
    /// publisher description.
    func kernelSummarizeEpisode(episodeID: String,
                                timeout: Duration = .seconds(30)) async -> (summary: String?, error: String?) {
        let projectionID = UUID(uuidString: episodeID)
        if let projectionID,
           let cached = episode(id: projectionID)?.summary,
           !cached.isEmpty {
            return (cached, nil)
        }
        let result = kernel?.dispatch(namespace: "podcast",
                                      body: ["op": "summarize_episode",
                                             "episode_id": episodeID])
        if case let .some(.failure(message)) = result {
            return (nil, message)
        }
        guard let projectionID else {
            return (nil, nil)
        }
        // React to the summary landing on the projected episode instead of
        // polling on a 300ms timer. `episode(id:)` reads `self.episodes`, so
        // the awaiter re-fires the instant `applyKernelState` stamps the
        // summary. Returns `nil` on timeout (e.g. Ollama offline).
        let summary = await awaitState(timeout: timeout, body: { [weak self] () -> String? in
            guard let summary = self?.episode(id: projectionID)?.summary,
                  !summary.isEmpty else { return nil }
            return summary
        })
        return (summary, nil)
    }

    // MARK: - Comments (NIP-22 / kind:1111)

    /// Subscribe to an episode's NIP-22 comments via the kernel. Rust opens a
    /// relay-pool subscription (no iOS WebSocket) and marks this episode as the
    /// one being viewed; inbound comments land on
    /// `podcastSnapshot.comments` via the reactive push seam.
    func kernelFetchComments(episodeID: String) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "fetch_comments",
                                "episode_id": episodeID])
    }

    /// Publish a NIP-22 comment for an episode via the kernel. Rust signs with
    /// the active user signer and routes through its relay pool — no secret
    /// bytes in app code. The comment is optimistically reflected on
    /// `podcastSnapshot.comments`.
    func kernelPostComment(episodeID: String, content: String) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "post_comment",
                                "episode_id": episodeID,
                                "content": content])
    }

    /// Publish a kind:1 agent-to-agent note via the kernel.
    /// Rust builds all NIP-10 tags and routes through the NMP relay pool.
    func kernelPublishAgentNote(
        recipientPubkeyHex: String,
        content: String,
        rootEventID: String? = nil,
        inboundEventID: String? = nil,
        rootATags: [String] = []
    ) {
        var body: [String: Any] = [
            "op": "publish_agent_note",
            "recipient_pubkey_hex": recipientPubkeyHex,
            "content": content
        ]
        if let root = rootEventID { body["root_event_id"] = root }
        if let inbound = inboundEventID { body["inbound_event_id"] = inbound }
        if !rootATags.isEmpty { body["root_a_tags"] = rootATags }
        kernel?.dispatch(namespace: "podcast", body: body)
    }

    // MARK: - Queue (podcast.queue namespace)

    /// Push an episode to the back of the Rust-owned Up Next queue.
    @discardableResult
    func kernelEnqueueLast(episodeID: UUID) -> DispatchResult? {
        kernelEnqueueLast(episodeID: episodeID.uuidString)
    }

    /// Push a raw episode id to the back of the Rust-owned Up Next queue.
    @discardableResult
    func kernelEnqueueLast(episodeID: String) -> DispatchResult? {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "enqueue", "episode_id": episodeID])
    }

    /// Push a bounded raw episode segment to the back of the Rust-owned queue.
    @discardableResult
    func kernelEnqueueSegmentLast(
        episodeID: String,
        startSeconds: Double?,
        endSeconds: Double
    ) -> DispatchResult? {
        var body: [String: Any] = [
            "op": "enqueue_segment",
            "episode_id": episodeID,
            "end_secs": endSeconds,
        ]
        if let startSeconds { body["start_secs"] = startSeconds }
        return kernel?.dispatch(namespace: "podcast.player", body: body)
    }

    /// Push an episode to the front of the Rust-owned Up Next queue (Play Next).
    @discardableResult
    func kernelEnqueueNext(episodeID: UUID) -> DispatchResult? {
        kernelEnqueueNext(episodeID: episodeID.uuidString)
    }

    /// Push a raw episode id to the front of the Rust-owned Up Next queue.
    @discardableResult
    func kernelEnqueueNext(episodeID: String) -> DispatchResult? {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "enqueue_next", "episode_id": episodeID])
    }

    /// Push a bounded raw episode segment to the front of the Rust-owned queue.
    @discardableResult
    func kernelEnqueueSegmentNext(
        episodeID: String,
        startSeconds: Double?,
        endSeconds: Double
    ) -> DispatchResult? {
        var body: [String: Any] = [
            "op": "enqueue_segment_next",
            "episode_id": episodeID,
            "end_secs": endSeconds,
        ]
        if let startSeconds { body["start_secs"] = startSeconds }
        return kernel?.dispatch(namespace: "podcast.player", body: body)
    }

    /// Remove all occurrences of an episode from the Rust-owned Up Next queue.
    func kernelDequeueEpisode(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.queue",
                         body: ["op": "remove", "episode_id": episodeID.uuidString])
    }

    /// Remove one Rust-owned queue slot from Up Next.
    func kernelDequeueQueueItem(queueSlotID: UUID) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: [
                            "op": "dequeue_slot",
                            "queue_slot_id": queueSlotID.uuidString,
                         ])
    }

    /// Reorder existing Rust-owned queue slots.
    func kernelReorderQueue(queueSlotIDs: [UUID]) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: [
                            "op": "reorder_queue",
                            "queue_slot_ids": queueSlotIDs.map(\.uuidString),
                         ])
    }

    /// Empty the Rust-owned Up Next queue.
    func kernelClearQueue() {
        kernel?.dispatch(namespace: "podcast.queue", body: ["op": "clear"])
    }

    // MARK: - Feedback (in-app TENEX project notes)

    /// Open the in-app feedback subscription via the kernel. Rust pushes a
    /// relay-pinned subscription to the feedback relay (no iOS WebSocket) for
    /// kind:1 + kind:513 events bearing the project `["a"]` coord; inbound
    /// events land on `podcastSnapshot.feedbackEvents` via the reactive push
    /// seam, and `FeedbackStore` rebuilds threads from them.
    func kernelFetchFeedback() {
        kernel?.dispatch(namespace: "podcast", body: ["op": "fetch_feedback"])
    }

    /// Publish a feedback note (kind:1) via the kernel. Rust builds all tags
    /// (project anchor, category, NIP-70 protected marker, NIP-10 reply
    /// markers), signs with the active user signer, and routes to the feedback
    /// relay with an explicit publish target (NMP AUTHs the write) — no secret
    /// bytes in app code, no iOS relay socket. `parentEventID` / `replyToPubkey`
    /// are nil for a new thread, set for a reply.
    func kernelPublishFeedback(
        category: String,
        content: String,
        parentEventID: String? = nil,
        replyToPubkey: String? = nil
    ) {
        var body: [String: Any] = [
            "op": "publish_feedback",
            "category": category,
            "content": content,
        ]
        if let parent = parentEventID { body["parent_event_id"] = parent }
        if let pk = replyToPubkey { body["reply_to_pubkey"] = pk }
        kernel?.dispatch(namespace: "podcast", body: body)
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

    /// Compile AI chapters + per-chapter summaries + ad spans for an episode
    /// in a single kernel LLM round-trip (namespace: podcast.chapters).
    ///
    /// The kernel owns all chapter + ad policy (D0): it detects boundaries,
    /// summaries, and ad spans from the cached transcript, persists results to
    /// `PodcastStore`, and bumps the snapshot rev so the projected `Episode`
    /// updates reactively.
    ///
    /// Two modes (selected by the kernel based on stored state):
    ///   - FULL — no publisher chapters yet: produce 4–12 chapters with
    ///     summaries + detect ad spans.
    ///   - ENRICH-ONLY — publisher chapters already exist: add per-chapter
    ///     summaries + detect ad spans; leave boundaries untouched.
    ///
    /// Idempotent: the kernel gates on whether ad detection has already run
    /// for the episode (mirrors the Swift `adSegments != nil` gate).
    /// Fire-and-forget: results arrive on the next snapshot push frame.
    func kernelCompileChapters(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.chapters",
                         body: ["op": "compile", "episode_id": episodeID.uuidString])
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
    ///
    /// Sends the typed mode + optional count to the Rust kernel (D7) so the
    /// kernel can honor `.latestN(N)` vs `.allNew` vs `.off` precisely.
    /// Also sends the legacy `enabled` bool for back-compat with any receiver
    /// that hasn't been updated yet.
    func kernelSetAutoDownload(podcastID: UUID, policy: AutoDownloadPolicy) {
        var body: [String: Any] = [
            "op": "set_auto_download",
            "podcast_id": podcastID.uuidString,
            "wifi_only": policy.wifiOnly,
            // Legacy bool — kept for back-compat with stale kernel versions.
            "enabled": policy.mode != .off
        ]
        switch policy.mode {
        case .off:
            body["mode"] = "off"
        case .allNew:
            body["mode"] = "all_new"
        case .latestN(let n):
            body["mode"] = "latest_n"
            body["count"] = n
        }
        kernel?.dispatch(namespace: "podcast", body: body)
    }

    // MARK: - Downloads

    /// Queue a download (namespace: podcast).
    /// Rust owns episode lookup, URL resolution, and unknown-id rejection.
    func kernelDownload(_ id: UUID) {
        kernelDownload(episodeID: id.uuidString)
    }

    /// Queue a download by raw episode id. Rust owns episode lookup and URL
    /// resolution for agent/tool callers.
    @discardableResult
    func kernelDownload(episodeID: String) -> DispatchResult? {
        DiagnosticLog.shared.append(
            level: .info, category: "dispatch",
            message: "download episode_id=\(episodeID)")
        return kernel?.dispatch(namespace: "podcast",
                                body: ["op": "download", "episode_id": episodeID])
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

    // MARK: - Local model downloads (unified queue, kind = local_model)

    /// Queue an on-device model download through the unified download queue
    /// (namespace: podcast). The kernel tags the item `local_model`, so the
    /// shared executor writes it to `LocalModels/<id>.litertlm` and it inherits
    /// resume / retry / background transfer.
    func kernelDownloadLocalModel(modelID: String, url: String) {
        DiagnosticLog.shared.append(
            level: .info, category: "dispatch",
            message: "download_local_model model_id=\(modelID)")
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "download_local_model", "model_id": modelID, "url": url])
    }

    /// Cancel an in-flight model download. Reuses the id-based cancel path
    /// (namespace: podcast.player) — the model id is the queue item's id.
    func kernelCancelLocalModelDownload(modelID: String) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "cancel_download", "episode_id": modelID])
    }

    // MARK: - Transcripts

    /// Report a completed transcript to the Rust kernel (M5.2 / slice 5a).
    ///
    /// Sends the full timed segment list so the kernel can produce RAG chunks
    /// with real `start_secs` / `end_secs` for seek-to-timestamp in search
    /// (slice 5a). Delegates to `KernelModel.sendTranscriptReport` which has
    /// access to the raw `podcastHandle` pointer.
    func kernelTranscriptReport(episodeID: UUID, transcript: Transcript, source: String? = nil) {
        kernel?.sendTranscriptReport(episodeID: episodeID, transcript: transcript, source: source)
    }

    /// Dispatch `podcast.knowledge.index_episode` for a single episode so the
    /// kernel KnowledgeStore gains that episode's transcript chunks immediately
    /// after the timed transcript has been mirrored via `kernelTranscriptReport`.
    ///
    /// Caller contract: `kernelTranscriptReport` MUST precede this call on the
    /// same episode — the kernel chunks the stored transcript text synchronously
    /// on the actor thread, so the text must already be in the store. A call
    /// without a prior report is a silent no-op (no transcript to chunk).
    ///
    /// Fire-and-forget + idempotent: `index_episode` deletes then re-upserts
    /// chunks, so repeat calls are safe. Live STT completion is one episode at a
    /// time; the single-episode bump cost (actor chunk pass + main-thread snapshot
    /// decode + embed spawn) is acceptable. No pacing is needed here — contrast
    /// with the launch-time backfill migration (slice 4) which dispatches N
    /// episodes in paced batches.
    func kernelIndexEpisodeKnowledge(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.knowledge",
                         body: ["op": "index_episode",
                                "episode_id": episodeID.uuidString])
    }

    /// Record one host-authored pipeline event onto an episode's Diagnostics
    /// log (download / transcript / chapters events the kernel emits itself; the
    /// iOS-capability stages — STT provider, RAG indexing, clip export, etc. —
    /// come through here). Fire-and-forget; no-op when the kernel is absent.
    func kernelRecordEpisodeEvent(
        episodeID: UUID,
        kind: String,
        severity: String,
        summary: String,
        details: [(String, String)] = []
    ) {
        kernel?.recordEpisodeEvent(
            episodeID: episodeID,
            kind: kind,
            severity: severity,
            summary: summary,
            details: details
        )
    }

    // MARK: - Diagnostics

    /// Fetch the kernel's per-episode pipeline event log for the Diagnostics
    /// sheet. Lazy single-episode read — these events are not part of the
    /// library snapshot. Returns `[]` when the kernel is unavailable.
    func kernelEpisodeEvents(_ id: UUID) -> [EpisodeAuditEvent] {
        kernel?.fetchEpisodeEvents(episodeID: id) ?? []
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
    /// library re-encode) per call rather than one per episode. `decision` is
    /// the raw `TriageDecision` rawValue, or `"none"` to clear a prior
    /// decision. The kernel owns triage (M5); the sole remaining Swift caller
    /// is `clearTriageDecision`, used when the user rescues an archived
    /// episode by playing it.
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

    /// Report the transient transcript-ingestion status for an episode. Rust
    /// derives `.ready` from the stored transcript; iOS reports the in-progress
    /// / failed / cleared states here. `status` is `"queued"` |
    /// `"fetching_publisher"` | `"transcribing"` | `"failed"` | `"none"`
    /// (clear). `message` carries the user-facing error for `"failed"`.
    @discardableResult
    func kernelSetEpisodeTranscriptStatus(
        episodeID: UUID,
        status: String,
        message: String?,
        provider: String? = nil
    ) -> DispatchResult? {
        kernelSetEpisodeTranscriptStatus(
            episodeID: episodeID.uuidString,
            status: status,
            message: message,
            provider: provider
        )
    }

    @discardableResult
    func kernelSetEpisodeTranscriptStatus(
        episodeID: String,
        status: String,
        message: String?,
        provider: String? = nil
    ) -> DispatchResult? {
        var body: [String: Any] = [
            "op": "set_episode_transcript_status",
            "episode_id": episodeID,
            "status": status,
        ]
        if let message { body["message"] = message }
        // Names the STT service on the `transcript.attempt` / `transcript.failed`
        // Diagnostics event so the log shows *which* provider is running.
        if let provider { body["provider"] = provider }
        return kernel?.dispatch(namespace: "podcast", body: body)
    }

    /// Record that the transcript pipeline deliberately *skipped* an episode,
    /// with a human-readable reason, in the kernel's per-episode Diagnostics
    /// event log. Unlike `kernelSetEpisodeTranscriptStatus`, this never mutates
    /// the durable transcript status — it only appends a `transcript.skipped`
    /// event so the Diagnostics sheet can explain why no transcription ran
    /// (category opt-out, AI transcription off, missing key, audio not on disk).
    func kernelRecordTranscriptSkip(episodeID: UUID, reason: String) {
        kernel?.dispatch(
            namespace: "podcast",
            body: [
                "op": "set_episode_transcript_status",
                "episode_id": episodeID.uuidString,
                "status": "skipped",
                "message": reason,
            ]
        )
    }

    // MARK: - LLM provider credentials (podcast.settings namespace)

    /// Push the current provider API keys into the Rust kernel so provider
    /// transport reads live values from the shared in-memory cache. Called on
    /// kernel attach and after every key save/delete.
    /// Rust can't read the Keychain directly — this is the only delivery path.
    func kernelSetProviderApiKeys() {
        var body: [String: Any] = ["op": "set_provider_api_keys"]
        do {
            if let key = try OpenRouterCredentialStore.apiKey() {
                body["open_router"] = key
            }
            if let key = try OllamaCredentialStore.apiKey() {
                body["ollama"] = key
            }
            if let key = try ElevenLabsCredentialStore.apiKey() {
                body["eleven_labs"] = key
            }
            if let key = try AssemblyAICredentialStore.apiKey() {
                body["assembly_ai"] = key
            }
            if let key = try PerplexityCredentialStore.apiKey() {
                body["perplexity"] = key
            }
        } catch {
            os_log(.error, log: OSLog(subsystem: "io.f7z.podcast", category: "AppStateStore"),
                   "Failed to resolve provider credentials for kernel: %{public}@", error.localizedDescription)
        }
        kernel?.dispatch(namespace: "podcast.settings", body: body)
    }

    // MARK: - App relays (podcast.settings namespace)
    //
    // Relay state is kernel-owned (NMP v0.2.1 `AppRelaySlot`), not `PodcastStore`.
    // `add_relay` upserts on URL, so `set_relay_role` is just an `add_relay` with
    // the new role. Reactivity is handled Rust-side: `settings_module.rs::execute`
    // emits the relay `ActorCommand` AND a companion `DispatchHostOp` that bumps
    // `handle.rev`, forcing the rev-gated snapshot push frame to rebuild and read
    // the just-mutated slot — so callers must NOT keep an optimistic local mirror.
    // Just dispatch and let `configuredRelays` refresh on the next projection.

    /// Add (or upsert the role of) a configured app relay. `role` must be a
    /// canonical NIP-65 role string (`read` | `write` | `both` | `indexer` |
    /// `both,indexer`); the kernel normalizes and validates it server-side.
    func kernelAddRelay(url: String, role: String) {
        kernel?.dispatch(namespace: "podcast.settings",
                         body: ["op": "add_relay", "url": url, "role": role])
    }

    /// Remove a configured app relay by URL. Idempotent server-side.
    func kernelRemoveRelay(url: String) {
        kernel?.dispatch(namespace: "podcast.settings",
                         body: ["op": "remove_relay", "url": url])
    }

    /// Change the NIP-65 role of an already-configured relay (upsert on URL).
    func kernelSetRelayRole(url: String, role: String) {
        kernel?.dispatch(namespace: "podcast.settings",
                         body: ["op": "set_relay_role", "url": url, "role": role])
    }

    // MARK: - NIP-F4 publishing (podcast.publish namespace)
    //
    // Canonical agent-owned podcast publishing. Rust owns the cryptography:
    // `create_owned_podcast` generates a per-podcast Nostr keypair, stamps
    // `owner_pubkey_hex` onto the podcast row, and registers the key so the
    // publish ops can sign. `publish_show` (kind:10154) and `publish_episode`
    // (kind:54) build + sign + broadcast NIP-F4 events to the relay pool;
    // `publish_author_claim` (kind:10064) lists every owned-podcast pubkey under
    // the active agent identity. These replace the legacy Swift NIP-74
    // (kind:30074/30075) builders.
    //
    // Fire-and-forget: the signed event id / naddr now lives in Rust's
    // `publish_state` and is surfaced via the snapshot projection, not returned
    // synchronously. Callers must NOT expect an event id back from dispatch.
    //
    // Field names verified against
    // apps/nmp-app-podcast/src/ffi/actions/publish_module.rs (PublishAction).

    /// Insert (or update) a podcast row in the Rust kernel store — the single
    /// source of truth. A feed-less row (`feedUrl: nil`) is an agent-owned / TTS
    /// show; a feed-backed row (`feedUrl` set) is an external-play placeholder.
    /// Idempotent on id — an enriched re-create updates the row in place. For
    /// owned podcasts this must run before `kernelCreateOwnedPodcast` for the
    /// same id (`create_owned_podcast` / `publish_show` `ok:false` without a
    /// row). `visibility` is the canonical `NostrVisibility` rawValue
    /// (`"public"` / `"private"`); `titleIsPlaceholder` marks a provisional
    /// feed-host fallback title awaiting metadata hydration.
    func kernelCreatePodcast(
        podcastId: String,
        title: String,
        description: String,
        author: String,
        feedUrl: String?,
        artworkUrl: String?,
        language: String?,
        categories: [String],
        visibility: String,
        titleIsPlaceholder: Bool
    ) {
        kernel?.dispatch(PodcastKernelAction.CreatePodcast(
            podcastId: podcastId,
            title: title,
            description: description,
            author: author,
            feedUrl: feedUrl,
            artworkUrl: artworkUrl,
            language: language,
            categories: categories,
            visibility: visibility,
            titleIsPlaceholder: titleIsPlaceholder
        ))
    }

    /// Insert (or update) an episode under a podcast in the Rust kernel store —
    /// the source of truth, so the episode survives the projection full-replace
    /// tick. `enclosureUrl` branches on scheme: a `file://` URL or bare absolute
    /// path → the audio is already on disk (Downloaded + local-path side-map);
    /// an `http(s)://` URL → a remote enclosure (NotDownloaded, fetched later by
    /// the download capability). `chapters` carry the parity fields (`image_url`
    /// for the mid-play artwork swap, `source_episode_id` for the source chip);
    /// `imageUrl` overrides the per-episode artwork. Fire-and-forget: the
    /// episode appears on the next projection tick.
    func kernelAddEpisode(
        podcastId: String,
        episodeId: String,
        title: String,
        enclosureUrl: String,
        description: String,
        durationSecs: Double?,
        imageUrl: String?,
        chapters: [KernelEpisodeChapterPayload],
        transcript: String?
    ) {
        kernel?.dispatch(PodcastKernelAction.AddEpisode(
            podcastId: podcastId,
            episodeId: episodeId,
            title: title,
            enclosureUrl: enclosureUrl,
            description: description,
            durationSecs: durationSecs,
            imageUrl: imageUrl,
            chapters: chapters,
            transcript: transcript
        ))
    }

    /// Claim ownership of a podcast for NIP-F4 publishing: Rust generates a
    /// per-podcast keypair and stamps `owner_pubkey_hex`. Must run before any
    /// `publish_show` / `publish_episode` for that podcast — those ops fail
    /// `ok:false ("podcast not owned")` if the key was never generated.
    func kernelCreateOwnedPodcast(podcastId: String) {
        kernel?.dispatch(namespace: "podcast.publish",
                         body: ["op": "create_owned_podcast", "podcast_id": podcastId])
    }

    /// Update an owned podcast's metadata in the kernel store and (when the
    /// podcast is public + nostr is enabled) re-publish its `kind:10154` show
    /// event. The kernel owns the publish gate — callers need not trigger a
    /// separate `publish_show` afterwards. Omitted (`nil`) fields keep their
    /// current value (partial update). `author` + `visibility` ride the op so
    /// the kernel store stays SSOT (otherwise the next snapshot push reverts a
    /// Swift-side edit / flip). `visibility` is the `NostrVisibility` rawValue.
    /// A private→public flip republishes the show in the same op (the kernel
    /// applies visibility before evaluating the gate).
    func kernelUpdateOwnedPodcast(
        podcastId: String,
        title: String?,
        description: String?,
        author: String?,
        artworkUrl: String?,
        visibility: String?
    ) {
        var body: [String: Any] = [
            "op": "update_owned_podcast",
            "podcast_id": podcastId,
        ]
        if let title { body["title"] = title }
        if let description { body["description"] = description }
        if let author { body["author"] = author }
        if let artworkUrl { body["artwork_url"] = artworkUrl }
        if let visibility { body["visibility"] = visibility }
        kernel?.dispatch(namespace: "podcast.publish", body: body)
    }

    /// Delete an owned podcast end-to-end via the kernel: publish a NIP-09
    /// (kind:5) deletion for the prior show event, drop the per-podcast key,
    /// and remove the podcast row + episodes from the kernel store. Replaces
    /// the old Swift `deletePodcast` → `kernelUnsubscribe` path (which leaked
    /// the per-podcast key and never published a deletion).
    func kernelDeleteOwnedPodcast(podcastId: String) {
        kernel?.dispatch(namespace: "podcast.publish",
                         body: ["op": "delete_owned_podcast", "podcast_id": podcastId])
    }

    /// Build, sign, and broadcast the NIP-F4 `kind:10154` show event for an
    /// owned podcast. Requires a prior `kernelCreateOwnedPodcast`.
    func kernelPublishShow(podcastId: String) {
        kernel?.dispatch(namespace: "podcast.publish",
                         body: ["op": "publish_show", "podcast_id": podcastId])
    }

    /// Build, sign, and broadcast the NIP-F4 `kind:54` episode event. Rust
    /// resolves the parent podcast (and its per-podcast key) from the episode,
    /// uploads the audio to Blossom when available, and falls back to the RSS
    /// enclosure URL otherwise. Requires the parent podcast to have been claimed
    /// via `kernelCreateOwnedPodcast`.
    func kernelPublishEpisode(episodeId: String) {
        kernel?.dispatch(namespace: "podcast.publish",
                         body: ["op": "publish_episode", "episode_id": episodeId])
    }

    /// Build, sign, and broadcast the NIP-F4 `kind:10064` author-claim event
    /// listing every owned-podcast pubkey under `agentPubkeyHex` (the active
    /// agent identity).
    func kernelPublishAuthorClaim(agentPubkeyHex: String) {
        kernel?.dispatch(namespace: "podcast.publish",
                         body: ["op": "publish_author_claim", "agent_pubkey_hex": agentPubkeyHex])
    }

}
