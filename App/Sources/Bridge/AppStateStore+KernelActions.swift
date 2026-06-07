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
        kern.dispatch(PodcastKernelAction.Subscribe(feedUrl: trimmed))
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
    func kernelSummarizeEpisode(episodeID: UUID,
                                timeout: Duration = .seconds(30)) async -> String? {
        if let cached = episode(id: episodeID)?.summary, !cached.isEmpty {
            return cached
        }
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "summarize_episode",
                                "episode_id": episodeID.uuidString])
        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline {
            if let summary = episode(id: episodeID)?.summary, !summary.isEmpty {
                return summary
            }
            try? await Task.sleep(for: .milliseconds(300))
        }
        return nil
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
    /// Passes the episode enclosure URL directly in the dispatch to avoid
    /// relying on Rust store lookup (which may not have the episode yet).
    func kernelDownload(_ id: UUID) {
        guard let episode = episode(id: id) else {
            os_log(.error, log: OSLog(subsystem: "io.f7z.podcast", category: "AppStateStore"),
                   "kernelDownload: episode not found: %{public}s", id.uuidString)
            return
        }

        let enclosureURL = episode.enclosureURL.absoluteString
        os_log(.debug, log: OSLog(subsystem: "io.f7z.podcast", category: "AppStateStore"),
               "kernelDownload: queuing episode=%{public}s url=%{public}s",
               id.uuidString, enclosureURL)
        DiagnosticLog.shared.append(
            level: .info, category: "dispatch",
            message: "download episode_id=\(id)")
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "download", "episode_id": id.uuidString, "url": enclosureURL])
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

    /// Report a completed transcript to the Rust kernel (M5.2).
    /// Delegates to `KernelModel.sendTranscriptReport` which has access to
    /// the raw `podcastHandle` pointer.
    func kernelTranscriptReport(episodeID: UUID, text: String) {
        kernel?.sendTranscriptReport(episodeID: episodeID, text: text)
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
