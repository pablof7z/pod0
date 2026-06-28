import Foundation
import Observation
import os
import os.signpost

// MARK: - KernelModel → AppState projection
//
// Observes both `KernelModel.library` (library-hash-gated: updates on
// subscribe/unsubscribe/mark-played/starred/download changes) and
// `KernelModel.podcastSnapshot` (content-hash-gated: updates on queue/
// settings/nowPlaying changes) using `withObservationTracking` so a single
// property change in either triggers a full projection pass — no fixed polling.
//
// Why two observables: `KernelModel` keeps them separate to avoid re-rendering
// list views at 4 Hz during playback.  The content hash that gates
// `podcastSnapshot` intentionally excludes library fields, so starred/played/
// download mutations only advance `library`, not `podcastSnapshot`.  If we
// watched only `podcastSnapshot` we would miss all library-only mutations.
//
// ID stability: Rust emits UUIDv5 strings for both PodcastId and EpisodeId
// (derived from feedURL|guid). `UUID(uuidString:)` parses them reliably,
// preserving foreign-key relationships across the projection.

extension AppStateStore {

    /// Call once from `AppMain` after both `store` and `kernelModel` exist.
    /// Uses `withObservationTracking` to drive the projection on every change
    /// to either `kernel.library` or `kernel.podcastSnapshot` — no fixed poll.
    @MainActor
    func attachKernel(_ kernel: KernelModel) {
        self.kernel = kernel
        // Forward the user's Nostr identity into the kernel so kernel-side
        // signing (podcast.social kind:0/1/9802, agent notes) has the active
        // local key. Re-syncs the current identity immediately (a key adopted
        // at launch, before the kernel attached, still reaches the kernel).
        identity.attachKernel(kernel)
        kernelObservationTask?.cancel()
        // Report which STT providers have a Keychain API key so the kernel's
        // STT fallback policy resolves `settings.effectiveSttProvider`
        // correctly from launch. Rust can't read the Keychain; this is the
        // only signal it has. Re-dispatched after every key save/delete.
        syncSTTKeysPresent()
        // Push OpenRouter and Ollama API keys to the Rust kernel's in-memory
        // provider registry. Keys are never persisted on either side; this
        // is re-dispatched after every Settings credential mutation so the
        // kernel always has the current Keychain values.
        kernelSetProviderApiKeys()
        // One-shot migration of legacy Swift-side user categories into the
        // kernel-owned `podcast_user_categories` substate (D0/D4). Guarded by a
        // UserDefaults flag so it runs exactly once; a no-op on fresh installs.
        migrateUserCategoriesToKernel()
        // One-shot migration of legacy per-category transcription settings into
        // the kernel-owned per-podcast transcription disabled set (D4/D7).
        migrateTranscriptionSettingsToKernel()
        migrateSocialNativeStoresToKernel()
        // One-shot re-backfill: dispatch `index_episode` for every episode
        // whose transcript the kernel already holds, populating the kernel
        // KnowledgeStore for the Search tab (Slice 4). Guarded by a
        // UserDefaults flag set AFTER the dispatch loop (idempotent).
        backfillKernelKnowledge()
        // Register the local LLM service callback so Rust can invoke Swift-side
        // inference through the loaded LiteRT-LM engine.
        let localService = localLLMService
        Task { @MainActor [weak self] in
            await localService.registerWithKernel(kernel)
            // Startup load: if a persisted role selection already points at a
            // local model, bring its engine up now (the callback was just
            // registered above, so inference is ready once the engine loads).
            self?.syncLocalEngine(for: self?.state.settings ?? Settings())
        }
        // Seed the Up Next queue from the kernel's persisted snapshot. The
        // handler may not be wired yet (setupPlaybackHandlers runs on .onAppear
        // which can fire after this task), so stash the items in pendingKernelQueue
        // as a fallback; setupPlaybackHandlers drains it on first access.
        let queueItems = Self.queueItems(from: kernel.podcastSnapshot)
        if !queueItems.isEmpty {
            if let handler = onQueueFromKernel {
                handler(queueItems)
            } else {
                pendingKernelQueue = queueItems
            }
        }
        kernelObservationTask = Task { @MainActor [weak self] in
            // Previous-tick `EpisodeSummary` cache, keyed by the wire `id`
            // string. Held as a local across loop iterations (this Task is the
            // ONLY caller of `applyKernelState`) so the projection can diff the
            // incoming library against the last one and skip `toEpisode` for
            // episodes whose summary is byte-for-byte unchanged. Kept here — NOT
            // as a stored property on `AppStateStore` — because
            // `AppStateStore.swift` is already over the 500-line hard limit
            // (see docs/BACKLOG.md line-limit-audit).
            var prevEpisodeSummaries: [String: EpisodeSummary] = [:]
            // `kernel.libraryGeneration` value behind the last FULL episode
            // rebuild. `-1` forces a full build on the first iteration
            // (generation starts at 0). When the current generation matches
            // this, `library` is byte-identical to that rebuild and the
            // projection takes the fast path — see `applyKernelState`.
            var lastProjectedLibraryGeneration = -1
            while !Task.isCancelled {
                // Apply current state FIRST, then arm the observation for the
                // next change. This eliminates the race where the kernel snapshot
                // advances between `attachKernel` returning and this Task's first
                // iteration — without this, `withObservationTracking` arms on an
                // already-final value and never fires, leaving the UI empty.
                if kernel.hasHydratedPodcastSnapshot {
                    self?.applyKernelState(
                        library: kernel.library,
                        snapshot: kernel.podcastSnapshot,
                        identity: kernel.kernelIdentity,
                        libraryGeneration: kernel.libraryGeneration,
                        prevEpisodeSummaries: &prevEpisodeSummaries,
                        lastProjectedLibraryGeneration: &lastProjectedLibraryGeneration)
                }
                // Diagnostic tap: one entry per snapshot tick. Logged at the
                // call site (not inside `applyKernelState`) so it covers both
                // the full and fast-path projections uniformly. The `message`
                // is an `@autoclosure`, so the `library.reduce` count never
                // runs unless debug logging is enabled — this is a hot path.
                DiagnosticLog.shared.append(
                    level: .debug, category: "kernel",
                    message: "snapshot tick rev=\(kernel.podcastSnapshot?.rev ?? 0) "
                        + "episodes=\(kernel.library.reduce(0) { $0 + $1.episodes.count })")
                // Suspend until kernel.library, kernel.podcastSnapshot, or
                // kernel.kernelIdentity changes. The identity write is
                // equality-gated in `KernelModel.apply`, so this arms only on a
                // genuine identity change (sign-in, handshake, or a new
                // resolved-profiles entry) — not at the 4 Hz playback emit rate.
                // withObservationTracking fires onChange once and returns; we loop
                // to re-arm continuously.
                await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                    withObservationTracking {
                        _ = kernel.library
                        _ = kernel.podcastSnapshot
                        _ = kernel.hasHydratedPodcastSnapshot
                        _ = kernel.kernelIdentity
                        // NOTE: `downloadSnapshot` is intentionally NOT observed
                        // here. Download-progress ticks update it ~1 Hz/download
                        // (without a global `rev` bump — see
                        // `nmp_app_podcast_download_report`); routing them through
                        // this full projection loop is exactly the cost we removed.
                        // They are handled by `downloadOverlayTask` below, which
                        // applies just the row overlay.
                    } onChange: {
                        continuation.resume()
                    }
                }
                guard !Task.isCancelled else { break }
            }
        }

        // Dedicated, cheap observation of ONLY `downloadSnapshot`: applies the
        // live download overlay onto `episodes` row-by-row as progress arrives,
        // without touching the library / decode / hash machinery above.
        downloadOverlayTask?.cancel()
        downloadOverlayTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                self?.applyDownloadOverlayOnly(active: kernel.downloadSnapshot?.active)
                await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                    withObservationTracking {
                        _ = kernel.downloadSnapshot
                    } onChange: {
                        continuation.resume()
                    }
                }
                guard !Task.isCancelled else { break }
            }
        }

        // Forward 1 Hz position ticks from applyAudioReport to UI consumers
        // (scrubber, Live Activity, lock-screen). The kernel's apply_writeback
        // (audio_report.rs) is the sole source of truth for position — Swift
        // only renders the kernel's value. The SQLite episode store no longer
        // persists or reads back position (#561).
        kernel.onPositionTick = { [weak self] _, pos in
            self?.onPositionTick?(pos)
        }
    }

    /// Apply ONLY the live download overlay onto `self.episodes`, off the heavy
    /// projection path. Driven by `downloadOverlayTask` observing
    /// `kernel.downloadSnapshot`. Mutates just the rows whose download state
    /// changed and skips the `@Observable` write entirely when nothing changed,
    /// so a progress tick never invalidates episode readers needlessly.
    private func applyDownloadOverlayOnly(active: [DownloadItemSnapshot]?) {
        var overlaid = self.episodes
        applyDownloadOverlay(to: &overlaid, active: active)
        if overlaid != self.episodes {
            self.episodes = overlaid
        }
    }

    /// Project the current kernel state into `AppState`.
    /// Takes `library` and `snapshot` separately because `KernelModel` gates
    /// them on different content hashes. `identity` carries the kernel's
    /// resolved-profiles map, merged into `nostrProfileCache` after the main
    /// projection lands.
    ///
    /// `libraryGeneration` is `KernelModel`'s monotonic library-reassignment
    /// counter. The observation that drives this method arms on `library`,
    /// `podcastSnapshot`, AND `kernelIdentity`, so it also fires when only the
    /// snapshot or identity changed while `library` stayed byte-identical (the
    /// common case: a mark-played echo, a 4 Hz-gated identity refresh, a
    /// settings/nowPlaying tick). On those ticks the whole library-derived pass
    /// (podcasts/subscriptions rebuild + the O(N) episode dict/loop/chapters)
    /// reproduces exactly what's already in `state`/`self.episodes`, so we skip
    /// it entirely via the fast path below. `lastProjectedLibraryGeneration`
    /// tracks the generation behind the current `self.episodes`.
    private func applyKernelState(
        library: [PodcastSummary],
        snapshot: PodcastUpdate?,
        identity: KernelIdentityProjection,
        libraryGeneration: Int,
        prevEpisodeSummaries: inout [String: EpisodeSummary],
        lastProjectedLibraryGeneration: inout Int
    ) {
        // Count is computed allocation-free (reduce, not flatMap) so the
        // signpost label adds no O(N) array copy to this hot path — the
        // os_signpost API defers FORMATTING, not argument evaluation.
        let applyInterval = signposter.beginInterval(
            "applyKernelState", "episodes=\(library.reduce(0) { $0 + $1.episodes.count })")
        defer { signposter.endInterval("applyKernelState", applyInterval) }

        // Perf: time the whole projection (full path AND the snapshot-only fast
        // path, since this sits above the fast-path branch). This is the largest
        // main-thread cost class — surfacing it in the Performance view lets us
        // see whether the fast-path guard is actually keeping no-op ticks cheap.
        let projectionStart = DispatchTime.now().uptimeNanoseconds
        defer {
            PerfMetrics.shared.record(
                .mainProjection,
                micros: Int((DispatchTime.now().uptimeNanoseconds &- projectionStart) / 1_000))
        }

        // ── Fast path: library unchanged since the last full projection ──────
        // `KernelModel` reassigns `library` (and bumps `libraryGeneration`)
        // only when `libraryMetaHash` changes. An unchanged generation proves
        // every podcast/episode field this projection reads is byte-identical
        // to the last full pass, so the entire library-derived rebuild is a
        // no-op that reproduces `self.episodes` exactly. Skip it and project
        // only the snapshot/identity-derived state (which is why the tick
        // fired). `prevEpisodeSummaries` is intentionally NOT touched here — it
        // already mirrors the unchanged library.
        if libraryGeneration == lastProjectedLibraryGeneration {
            applyKernelSnapshotOnlyState(
                library: library, snapshot: snapshot, identity: identity)
            return
        }
        lastProjectedLibraryGeneration = libraryGeneration

        var next = state

        // ── Podcasts + subscriptions ──────────────────────────────────────
        var podcasts: [Podcast] = []
        var subscriptions: [PodcastSubscription] = []

        for summary in library {
            guard let uuid = UUID(uuidString: summary.id) else { continue }
            let feedURL = summary.feedUrl.flatMap { URL(string: $0) }
            // Agent-owned rows now live in the Rust store as SSOT and project
            // back here. Without round-tripping `ownerPubkeyHex` /
            // `nostrVisibility` the wholesale `next.podcasts` replace below would
            // rebuild every row with no owner, wiping owned-podcast detection
            // (`listOwnedPodcasts` filters on `ownerPubkeyHex != nil`) and the
            // publish gate. A feed-less row is just a podcast with `feedURL ==
            // nil` — no separate kind discriminator.
            let visibility = Podcast.NostrVisibility(rawValue: summary.nostrVisibility) ?? .public
            podcasts.append(Podcast(
                id: uuid,
                feedURL: feedURL,
                title: summary.title,
                author: summary.author ?? "",
                imageURL: summary.artworkUrl.flatMap { URL(string: $0) },
                description: summary.description ?? "",
                lastRefreshedAt: summary.lastRefreshedAt.map {
                    Date(timeIntervalSince1970: TimeInterval($0) / 1000)
                },
                titleIsPlaceholder: summary.titleIsPlaceholder,
                ownerPubkeyHex: summary.ownerPubkeyHex,
                nostrVisibility: visibility
            ))
            if summary.isSubscribed {
                // `cellularAllowed` is projected from Rust's
                // `auto_download_cellular_allowed` set; absent (false) means
                // the default Wi-Fi-only behaviour. Round-trip the flag so a
                // user who turned off Wi-Fi-only doesn't find it silently
                // re-enabled after the next kernel snapshot.
                //
                // D7: prefer the typed `autoDownloadMode` + `autoDownloadCount`
                // fields when present. Fall back to the legacy bool for kernels
                // that haven't shipped the typed projection yet.
                let wifiOnly = !summary.cellularAllowed
                let autoDownload: AutoDownloadPolicy
                switch summary.autoDownloadMode {
                case "all_new":
                    autoDownload = AutoDownloadPolicy(mode: .allNew, wifiOnly: wifiOnly)
                case "latest_n":
                    let n = summary.autoDownloadCount > 0 ? summary.autoDownloadCount : 3
                    autoDownload = AutoDownloadPolicy(mode: .latestN(n), wifiOnly: wifiOnly)
                case "off":
                    autoDownload = AutoDownloadPolicy(mode: .off, wifiOnly: wifiOnly)
                default:
                    // Legacy fallback: old kernel only projects the bool.
                    autoDownload = summary.autoDownload
                        ? AutoDownloadPolicy(mode: .allNew, wifiOnly: wifiOnly)
                        : AutoDownloadPolicy(mode: .off, wifiOnly: wifiOnly)
                }
                subscriptions.append(PodcastSubscription(
                    podcastID: uuid,
                    autoDownload: autoDownload,
                    notificationsEnabled: summary.notificationsEnabled
                ))
            }
        }
        // Preserve the Unknown sentinel row so legacy foreign keys resolve.
        if !podcasts.contains(where: { $0.id == Podcast.unknownID }) {
            podcasts.append(Podcast.unknown)
        }
        next.podcasts = podcasts
        next.subscriptions = subscriptions

        // ── Episodes (summary-level diff) ─────────────────────────────────
        // The kernel re-emits the FULL library on every content-changing tick,
        // even when a single field on a single episode changed. Naively mapping
        // every `EpisodeSummary` through `toEpisode` reallocates all N `Episode`
        // structs AND runs `toEpisode`'s per-episode work (incl. a main-thread
        // file stat for every downloaded episode) on each tick.
        //
        // Diff at the SUMMARY level instead: `EpisodeSummary` is `Equatable`, so
        // a summary byte-for-byte identical to the previous tick's yields a
        // reusable `Episode` with no `toEpisode` call. Only NEW or CHANGED
        // summaries pay the mapping cost — the common case (one mutation on one
        // episode) is a single `toEpisode` call, not N.
        //
        // NOTE: an Episode-LEVEL diff (map all summaries, then compare Episodes)
        // would be pointless here — it still calls `toEpisode` for every summary
        // to produce the comparison value, saving no allocation and no stat. The
        // win comes only from gating `toEpisode` on the summary comparison.
        //
        // REUSE INVARIANT: reusing the prior `Episode` (rather than re-deriving
        // it) is behaviour-preserving ONLY because every Swift writer that edits
        // `state.episodes` between ticks either (a) dispatches to the kernel so
        // the next `EpisodeSummary` differs and we re-derive (starred, played,
        // metadataIndexed, transcriptStatus, adSegments, triageDecision), (b) is
        // the chapters fallback below — deliberately preserved, or (c) writes a
        // value `toEpisode` would itself produce (`.clearFailed → .notDownloaded`,
        // which a nil `downloadPath` also yields). If a NEW Swift-only writer
        // sets a field absent from `EpisodeSummary` and not re-derivable, reuse
        // would keep it stale — route such a field through the kernel (a) or the
        // chapters-style merge (b) instead. The old full-rebuild masked this by
        // wiping every Swift mutation back to kernel truth each content tick.
        //
        // Parent-id stability: reuse keys on `EpisodeSummary` equality alone, but
        // `toEpisode` also takes the parent `summary.id`. Episode ids are
        // UUIDv5(feedURL|guid) — feed-bound — so an episode cannot reparent
        // without its own id (and thus its summary key) changing. No risk of a
        // stale `podcastID` surviving reuse.
        let priorEpisodesByID = Dictionary(
            self.episodes.map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first }
        )
        var episodes: [Episode] = []
        episodes.reserveCapacity(library.reduce(0) { $0 + $1.episodes.count })
        var nextEpisodeSummaries: [String: EpisodeSummary] = [:]
        nextEpisodeSummaries.reserveCapacity(prevEpisodeSummaries.count)
        for summary in library {
            for ep in summary.episodes {
                nextEpisodeSummaries[ep.id] = ep
                // Reuse the prior `Episode` only when the wire summary is
                // unchanged AND we still hold the mapped value. `toEpisode`
                // parses the id; an unparseable id produced no prior episode and
                // re-mapping it just returns nil again, so the lookup also
                // naturally skips those.
                if prevEpisodeSummaries[ep.id] == ep,
                   let parsedID = UUID(uuidString: ep.id),
                   let prior = priorEpisodesByID[parsedID] {
                    // Reused summary: apply the live nowPlaying position as a
                    // render-only overlay (the kernel remains the sole writer).
                    // The kernel excludes position_secs from the library content hash
                    // so the summary is never "changed" by a position tick alone —
                    // the prior episode would otherwise keep its stale playbackPosition.
                    // Guard on isPlaying: nowPlaying can be non-nil with positionSecs>0
                    // from a restored-but-paused kernel state, which would incorrectly
                    // re-apply the live overlay on a fresh launch. Restrict the live
                    // overlay to active playback; paused/restored positions arrive via
                    // the kernel's ep.position_secs projection (below), not the overlay.
                    var reused = prior
                    // Always project ep.position_secs from the kernel. The prior episode
                    // carries 0 on cold launch (SQLite no longer persists position — #561)
                    // and is overridden by the live kernel value here on every tick.
                    //
                    // nil-coalescing to 0: the kernel only sets playbackPositionSecs when
                    // position_secs > 0.0 (snapshot_library.rs line 73). A nil value means
                    // position is at the start; 0 is correct.
                    reused.playbackPosition = ep.playbackPositionSecs ?? 0
                    // Overlay the live kernel position (isPlaying only) so the
                    // scrubber stays current during active playback. This is render-only
                    // and never written to disk.
                    if kernel?.nowPlaying?.isPlaying == true,
                       kernel?.nowPlaying?.episodeId == ep.id,
                       let livePos = kernel?.nowPlaying?.positionSecs,
                       livePos > reused.playbackPosition {
                        reused.playbackPosition = livePos
                    }
                    episodes.append(reused)
                } else if let episode = ep.toEpisode(podcastIdString: summary.id) {
                    // Kernel ep.position_secs is the sole position source.
                    // No Swift fallback recovery — a stale Swift row must not
                    // override the kernel's authoritative value.
                    episodes.append(episode)
                }
            }
        }
        prevEpisodeSummaries = nextEpisodeSummaries
        // Also include episodes from the active queue (snapshot may lag library
        // if only library changed, but queue episodes still need to resolve).
        for ep in snapshot?.queue ?? [] {
            let podcastIdString = ep.podcastId ?? Podcast.unknownID.uuidString
            if let episode = ep.toEpisode(podcastIdString: podcastIdString),
               !episodes.contains(where: { $0.id == episode.id }) {
                episodes.append(episode)
            }
        }

        // Assign the projected list to the live `self.episodes` stored property
        // inside the batch below (episodes no longer round-trip through `state`).
        var projectedEpisodes = episodes
        // Overlay live download states (.queued / .downloading / .failed) from
        // the kernel DownloadQueue projection. `toEpisode` only knows two states
        // (.downloaded when downloadPath != nil, .notDownloaded otherwise); the
        // active-queue snapshot carries the in-progress states that toEpisode
        // cannot see. Must run after the episode array is fully built so it also
        // catches episodes that went through the unchanged-summary reuse path.
        applyDownloadOverlay(to: &projectedEpisodes, active: snapshot?.downloads?.active)

        // Project the snapshot/identity-derived state (settings + last-played).
        // Shared verbatim with the snapshot-only fast path.
        projectSnapshotDerivedState(into: &next, snapshot: snapshot)

        // Batch every state-mutating write below so the derived work the
        // `state.didSet` chain triggers (episode-projection rebuild, persist,
        // widget reload) runs ONCE on batch exit instead of per-write.
        //
        // Without the batch this method double-recomputed the episode
        // projections on every content tick: `state = next` fires
        // `handleStateDidSet`, which recomputes immediately when the array
        // fingerprint changed, and the explicit `invalidateEpisodeProjections()`
        // below then forced a second, redundant rebuild. Inside the batch both
        // paths only *set* `deferredEpisodeProjectionRebuild`, and
        // `flushDeferredMutationWork()` collapses them into a single recompute.
        //
        // The explicit `invalidateEpisodeProjections()` stays load-bearing:
        // `episodesFingerprintChanged` only catches count / first-id / last-id
        // changes, so a same-count *merge* — e.g. the kernel flipping
        // `played: false → true` at natural end (now the canonical
        // mark-played-at-end path, see `onItemEnd`), or clearing a
        // `downloadPath` on delete-after-played — slips past the fingerprint.
        // Without the explicit invalidation the in-progress carousel keeps a
        // just-finished episode, the unplayed badge stays stale, and the
        // "Downloaded" filter chip lingers after a delete. `applyKernelState`
        // is content-gated (the observation arms on hash-gated
        // `library`/`snapshot`/`identity`, not the 4 Hz emit rate) and already
        // does the full O(N) episode walk above, so this recompute fires only
        // on a real content change and adds no new cost class.

        performMutationBatch {
            state = next
            self.episodes = projectedEpisodes

            // Clips are read directly from the kernel snapshot on demand via
            // `kernelProjectedClips()` (see `AppStateStore+Clips`); there is no
            // longer a Swift-side `state.clips` to project into.

            invalidateEpisodeProjections()

            // ── Kernel-resolved profiles → nostrProfileCache ──────────────
            // Additive merge of `projections.resolved_profiles` (NMP v0.2.0+).
            // Run AFTER `state = next` so the snapshot taken at the top of this
            // method doesn't clobber the inserts. Routed through
            // `setNostrProfile` (createdAt = 0): its
            // `existing.fetchedFromCreatedAt >= 0` guard makes this idempotent
            // and never downgrades a real relay-sourced kind:0 (createdAt > 0),
            // while still seeding pubkeys the cache hasn't seen. This is the
            // delivery half of reference-first profile resolution: display
            // surfaces `claimNostrProfiles(_:consumer:)` the pubkeys they
            // render, the kernel resolves each kind:0 over its relay pool, and
            // the result lands here on the next push frame. The bespoke
            // `NostrProfileFetcher` remains only for `NostrAgentResponder`'s
            // synchronous prompt-building window and the approval-enrich
            // snapshot — neither of which an async push can satisfy.
            mergeResolvedProfiles(identity.resolvedProfiles)
            self.identity.applyKernelIdentity(
                handshake: identity.bunkerHandshake,
                activeNpub: identity.activeNpub,
                pubkeyHex: identity.activeAccount,
                isRemoteSigner: identity.isRemoteSigner,
                displayName: identity.activeDisplayName,
                name: identity.activeName,
                about: identity.activeAbout,
                pictureUrl: identity.activePictureUrl)
        }

        // After the batch flushes: the widget path reads only `snapshot.widget`
        // (the kernel-owned projection), so running it here — once the single
        // deferred recompute has already landed — is correct and keeps it from
        // being counted as another batched mutation.
        onQueueFromKernel?(Self.queueItems(from: snapshot))
        onNowPlayingSnapshot?(snapshot)
    }

    /// Snapshot-only projection: the work `applyKernelState` runs when the
    /// kernel `library` is byte-identical to the last full projection (the
    /// tick fired solely on a `podcastSnapshot`/`kernelIdentity` change). It
    /// reprojects ONLY the snapshot/identity-derived state — settings,
    /// last-played, resolved profiles, and the now-playing/widget hook —
    /// and deliberately skips the entire library-derived rebuild:
    ///
    ///   • `podcasts`/`subscriptions` would rebuild identically from the
    ///     unchanged `library`, so they stay as already stored in `state`.
    ///   • The episode dict + reuse loop + chapters fallback would reproduce
    ///     `self.episodes` element-for-element (every summary is unchanged, so
    ///     every entry is the reused prior `Episode`), so `self.episodes` is
    ///     left untouched — no allocation, no copy.
    ///   • `invalidateEpisodeProjections()` is skipped: the episode array is
    ///     unchanged, so the cached projections are still valid. (This also
    ///     drops the downstream O(N) projection recompute that the full path's
    ///     explicit invalidation forces.)
    ///
    /// `prevEpisodeSummaries` and `lastProjectedLibraryGeneration` are NOT
    /// advanced — both still correctly describe the unchanged library.
    private func applyKernelSnapshotOnlyState(
        library: [PodcastSummary],
        snapshot: PodcastUpdate?,
        identity: KernelIdentityProjection
    ) {
        var next = state
        projectSnapshotDerivedState(into: &next, snapshot: snapshot)
        // Re-apply the download overlay even on the fast path: download progress
        // ticks bump `rev` without changing the library, so they arrive here.
        // Without this, an in-progress download never shows its progress ring.
        var overlaidEpisodes = self.episodes
        applyDownloadOverlay(to: &overlaidEpisodes, active: kernel?.downloadSnapshot?.active)
        performMutationBatch {
            state = next
            self.episodes = overlaidEpisodes
            // Clips are read on demand from the kernel snapshot via
            // `kernelProjectedClips()`; no Swift-side projection needed.
            mergeResolvedProfiles(identity.resolvedProfiles)
            self.identity.applyKernelIdentity(
                handshake: identity.bunkerHandshake,
                activeNpub: identity.activeNpub,
                pubkeyHex: identity.activeAccount,
                isRemoteSigner: identity.isRemoteSigner,
                displayName: identity.activeDisplayName,
                name: identity.activeName,
                about: identity.activeAbout,
                pictureUrl: identity.activePictureUrl)
        }
        onQueueFromKernel?(Self.queueItems(from: snapshot))
        onNowPlayingSnapshot?(snapshot)
    }

    private static func queueItems(from snapshot: PodcastUpdate?) -> [QueueItem] {
        (snapshot?.queue ?? []).compactMap { row -> QueueItem? in
            guard let id = UUID(uuidString: row.id) else { return nil }
            let slotID = row.queueSlotId.flatMap(UUID.init(uuidString:)) ?? UUID()
            return QueueItem(
                id: slotID,
                episodeID: id,
                startSeconds: row.queueStartSecs,
                endSeconds: row.queueEndSecs
            )
        }
    }

    /// Fold the kernel's resolved-profiles map into `nostrProfileCache`. Each
    /// entry becomes a minimal `NostrProfileMetadata` (display → displayName,
    /// pictureUrl → picture) so agent-conversation views resolve a name and
    /// avatar without a Swift-side relay round-trip. Idempotent via the
    /// `setNostrProfile` createdAt guard.
    /// Overlay live download states onto an episode array from the kernel
    /// `DownloadQueueSnapshot`. `toEpisode` only knows `.downloaded` /
    /// `.notDownloaded`; in-progress states (`.queued`, `.downloading`,
    /// `.failed`) live only in the snapshot's active-queue projection.
    ///
    /// Idempotent: episodes whose `downloadState` is already `.downloaded`
    /// are left untouched — a completed file on disk wins over queue state.
    private func applyDownloadOverlay(to episodes: inout [Episode], active: [DownloadItemSnapshot]?) {
        let active = active ?? []
        // Extract local-model rows for the Providers → Local UI (live progress).
        // Done before the episode guard so it also *clears* when the last model
        // download finishes (active goes empty → row flips downloaded).
        let modelRows = active.filter { $0.kind == .localModel }
        // `uniquingKeysWith` (keep last) rather than `uniqueKeysWithValues`: the
        // kernel's active-download list can momentarily carry two rows for the
        // same id (e.g. a requeued/retried download). A UI projection must never
        // trap on data shape — `uniqueKeysWithValues` fatalErrors on a dup key,
        // which crashed the app mid-playback. Last row wins (most recent state).
        let newModelDownloads = Dictionary(
            modelRows.map { ($0.episodeId, $0) },
            uniquingKeysWith: { _, last in last })
        if newModelDownloads != localModelDownloads {
            localModelDownloads = newModelDownloads
        }
        guard !active.isEmpty else { return }
        // Episode rows only — the unified queue also carries non-episode
        // downloads (e.g. local models) whose ids are not episode UUIDs.
        let byID = Dictionary(
            active
                .filter { $0.kind == .episode }
                .map { ($0.episodeId.uppercased(), $0) },
            uniquingKeysWith: { _, last in last })
        for idx in episodes.indices {
            let key = episodes[idx].id.uuidString.uppercased()
            guard let dl = byID[key] else { continue }
            // Don't downgrade a completed download.
            if case .downloaded = episodes[idx].downloadState { continue }
            switch dl.state {
            case "queued", "paused":
                episodes[idx].downloadState = .queued
            case "active":
                let bytesWritten: Int64? = dl.totalBytes.map { Int64(dl.progress * Double($0)) }
                episodes[idx].downloadState = .downloading(
                    progress: dl.progress,
                    bytesWritten: bytesWritten)
            case "failed":
                episodes[idx].downloadState = .failed(message: dl.error ?? "Download failed")
            default:
                break
            }
        }
    }

    private func mergeResolvedProfiles(_ profiles: [String: ResolvedProfile]) {
        for (pubkey, profile) in profiles {
            // Skip empty rows — no name and no picture is nothing to surface,
            // and inserting one would only mask a later richer fetch.
            guard profile.displayName != nil || profile.pictureUrl != nil else { continue }
            setNostrProfile(NostrProfileMetadata(
                pubkey: pubkey,
                name: nil,
                displayName: profile.displayName,
                about: nil,
                picture: profile.pictureUrl,
                nip05: nil,
                fetchedFromCreatedAt: 0
            ))
        }
    }
}

// `EpisodeSummary.toEpisode` / `ChapterSummary.toChapter` wire-to-domain
// mapping lives in `EpisodeSummary+Projection.swift` (split out to keep this
// file under the 500-line hard limit — see the `kernelprojection-split`
// backlog item).
