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
        // which can fire after this task), so stash the IDs in pendingKernelQueue
        // as a fallback; setupPlaybackHandlers drains it on first access.
        let queueIDs = (kernel.podcastSnapshot?.queue ?? []).compactMap { UUID(uuidString: $0.id) }
        if !queueIDs.isEmpty {
            if let handler = onQueueFromKernel {
                handler(queueIDs)
                onQueueFromKernel = nil
            } else {
                pendingKernelQueue = queueIDs
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
                self?.applyKernelState(
                    library: kernel.library,
                    snapshot: kernel.podcastSnapshot,
                    identity: kernel.kernelIdentity,
                    libraryGeneration: kernel.libraryGeneration,
                    prevEpisodeSummaries: &prevEpisodeSummaries,
                    lastProjectedLibraryGeneration: &lastProjectedLibraryGeneration)
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

        // Forward 1 Hz position ticks from applyAudioReport directly into the
        // debounce cache. Bypasses withObservationTracking (which is unreliable
        // for sub-tick mutation) in favour of a synchronous callback on the
        // MainActor — same thread where applyAudioReport fires.
        kernel.onPositionTick = { [weak self] idStr, pos in
            guard let self, let id = UUID(uuidString: idStr) else { return }
            self.setEpisodePlaybackPosition(id, position: pos)
            self.onPositionTick?(pos)
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
                let autoDownload: AutoDownloadPolicy = summary.autoDownload
                    ? AutoDownloadPolicy(mode: .allNew, wifiOnly: !summary.cellularAllowed)
                    : AutoDownloadPolicy(mode: .off, wifiOnly: !summary.cellularAllowed)
                subscriptions.append(PodcastSubscription(
                    podcastID: uuid,
                    autoDownload: autoDownload
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
                    // Reused summary: apply position cache and nowPlaying as overlays.
                    // The kernel excludes position_secs from the library content hash
                    // so the summary is never "changed" by a position tick alone —
                    // the prior episode would otherwise keep its stale playbackPosition.
                    // Guard on isPlaying: nowPlaying can be non-nil with positionSecs>0
                    // from a restored-but-paused kernel state, which would incorrectly
                    // set the position on a fresh launch. Restrict to active playback
                    // only; paused/restored positions reach us via Persistence instead.
                    var reused = prior
                    let livePos: TimeInterval? =
                        positionCache[parsedID]
                        ?? (kernel?.nowPlaying?.isPlaying == true && kernel?.nowPlaying?.episodeId == ep.id
                            ? kernel?.nowPlaying?.positionSecs : nil)
                    if let livePos, livePos > reused.playbackPosition {
                        reused.playbackPosition = livePos
                    }
                    episodes.append(reused)
                } else if var episode = ep.toEpisode(podcastIdString: summary.id) {
                    if Self.synchronousPositionFlushForUITests,
                       !CommandLine.arguments.contains("--UITestSeedRelaunch") {
                        // Non-relaunch UITest: zero whatever position the kernel
                        // reports. UITestSeeder writes position_secs=0 to
                        // podcasts.json, but the kernel's player-state restoration
                        // writes the prior session's position back to that file
                        // during nmp_app_start before Swift sees the first snapshot.
                        // That makes ep.playbackPositionSecs non-zero regardless of
                        // UITestSeeder's seed. Zeroing unconditionally here ensures
                        // every fresh-seed test launch sees "Play", not "Resume".
                        episode.playbackPosition = 0
                    } else if episode.playbackPosition == 0,
                              let parsedID = UUID(uuidString: ep.id) {
                        // The kernel only writes ep.position_secs on explicit
                        // PersistPosition actions (seek/skip while paused). For
                        // episodes rebuilt from a changed summary (e.g. RSS metadata
                        // refresh), ep.position_secs is still 0 even though the user
                        // listened part-way. Recover from positionCache first (most
                        // accurate), then from the prior Episode (last flushed value).
                        let recovered = positionCache[parsedID]
                            ?? priorEpisodesByID[parsedID]?.playbackPosition
                            ?? 0
                        if recovered > 0 { episode.playbackPosition = recovered }
                    }
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

        // ── Legacy Swift chapters fallback ────────────────────────────────
        // For REUSED episodes this is a no-op — they already carry their merged
        // chapters from the tick that first mapped them. It exists for the
        // NEW/CHANGED episodes that just came out of `toEpisode`.
        //
        // Rust now has a real `podcast.chapters.compile` path and projects
        // stored chapters. The remaining exception is the legacy Swift
        // `AIChapterCompiler`, which still writes through `setEpisodeChapters`
        // without dispatching to Rust. Until those call sites move to the kernel
        // action, keep prior Swift-written chapters when Rust projects none so
        // they do not flash empty after a feed-refresh projection.
        // Tracked in docs/BACKLOG.md.
        //
        // Reuses `priorEpisodesByID` from the diff above. Reused (unchanged)
        // episodes already carry their merged chapters from the tick that first
        // mapped them, so this only does real work for the newly-mapped ones —
        // but running it over the whole array is harmless and keeps the fallback
        // a single, obviously-correct pass.
        for idx in episodes.indices {
            guard let prior = priorEpisodesByID[episodes[idx].id] else { continue }
            if episodes[idx].chapters?.isEmpty != false {
                episodes[idx].chapters = prior.chapters
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
                isRemoteSigner: identity.isRemoteSigner)
        }

        // After the batch flushes: the widget path reads only `snapshot.widget`
        // (the kernel-owned projection), so running it here — once the single
        // deferred recompute has already landed — is correct and keeps it from
        // being counted as another batched mutation.
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
        // Propagate the live nowPlaying position through the debounce cache so
        // store.episode(id:) returns the correct playhead for the episode detail.
        // The episode-summary hash excludes positionSecs, so the full projection
        // loop never runs on a position-only event; the kernel's nowPlaying is
        // the ONLY in-process source of the current position. We call
        // setEpisodePlaybackPosition BEFORE capturing self.episodes below so
        // the eager flush (first call, or after 30 s) lands in self.episodes
        // before overlaidEpisodes is built.
        //
        // Guard on isPlaying: the kernel can restore a non-nil nowPlaying with
        // positionSecs > 0 from its own internal state after a fresh launch
        // (independent of what UITestSeeder writes to podcasts.json). Without
        // this guard, the stale restored position writes through to self.episodes
        // and the episode detail shows "Resume" on a clean launch instead of
        // "Play". The "just paused" case is already handled by the explicit
        // flushPendingPositions() call on pause and by the background flush
        // observer, so restricting this path to isPlaying=true is safe.
        if let np = kernel?.nowPlaying,
           np.isPlaying,
           np.positionSecs > 0,
           let idStr = np.episodeId,
           let id = UUID(uuidString: idStr) {
            setEpisodePlaybackPosition(id, position: np.positionSecs)
        }
        // Re-apply the download overlay even on the fast path: download progress
        // ticks bump `rev` without changing the library, so they arrive here.
        // Without this, an in-progress download never shows its progress ring.
        var overlaidEpisodes = self.episodes
        applyDownloadOverlay(to: &overlaidEpisodes, active: kernel?.downloadSnapshot?.active)
        performMutationBatch {
            state = next
            self.episodes = overlaidEpisodes
            mergeResolvedProfiles(identity.resolvedProfiles)
            self.identity.applyKernelIdentity(
                handshake: identity.bunkerHandshake,
                activeNpub: identity.activeNpub,
                pubkeyHex: identity.activeAccount,
                isRemoteSigner: identity.isRemoteSigner)
        }
        onNowPlayingSnapshot?(snapshot)
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
        let newModelDownloads = Dictionary(uniqueKeysWithValues: modelRows.map { ($0.episodeId, $0) })
        if newModelDownloads != localModelDownloads {
            localModelDownloads = newModelDownloads
        }
        guard !active.isEmpty else { return }
        // Episode rows only — the unified queue also carries non-episode
        // downloads (e.g. local models) whose ids are not episode UUIDs.
        let byID = Dictionary(uniqueKeysWithValues: active
            .filter { $0.kind == .episode }
            .map { ($0.episodeId.uppercased(), $0) })
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
            guard profile.display != nil || profile.pictureUrl != nil else { continue }
            setNostrProfile(NostrProfileMetadata(
                pubkey: pubkey,
                name: nil,
                displayName: profile.display,
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
