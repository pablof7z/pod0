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
        // One-shot backfill of pre-#215 agent-generated episodes. Runs
        // SYNCHRONOUSLY here — before the observation `Task` below is created —
        // so it reads `state.episodes` while it still holds the persisted,
        // pre-kernel set. The first `applyKernelState` (inside that Task) does a
        // full-replace from the kernel projection, so any synthetic episode not
        // yet registered in the kernel would be wiped before we could see it.
        backfillSyntheticEpisodes()
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
                    } onChange: {
                        continuation.resume()
                    }
                }
                guard !Task.isCancelled else { break }
            }
        }
    }

    // `backfillSyntheticEpisodes()` (the one-shot pre-#215 synthetic-episode
    // migration called from `attachKernel`) lives in
    // `AppStateStore+SyntheticBackfill.swift` (split out to keep this file
    // under the 500-line hard limit — see the `kernelprojection-split`
    // backlog item).

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
            // Synthetic (agent-owned) rows now live in the Rust store as SSOT
            // and project back here. Without round-tripping `kind` /
            // `ownerPubkeyHex` / `nostrVisibility` the wholesale `next.podcasts`
            // replace below would rebuild every row as `.rss` with no owner,
            // wiping owned-podcast detection (`listOwnedPodcasts` filters on
            // `ownerPubkeyHex != nil`) and the publish gate.
            let kind: Podcast.Kind = summary.kind == "synthetic" ? .synthetic : .rss
            let visibility = Podcast.NostrVisibility(rawValue: summary.nostrVisibility) ?? .public
            podcasts.append(Podcast(
                id: uuid,
                kind: kind,
                feedURL: feedURL,
                title: summary.title,
                author: summary.author ?? "",
                imageURL: summary.artworkUrl.flatMap { URL(string: $0) },
                description: summary.description ?? "",
                ownerPubkeyHex: summary.ownerPubkeyHex,
                nostrVisibility: visibility
            ))
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
                    episodes.append(prior)
                } else if let episode = ep.toEpisode(podcastIdString: summary.id) {
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

        // ── Chapters fallback (sole re-derived preserved-state field) ─────
        // For REUSED episodes this is a no-op — they already carry their merged
        // chapters from the tick that first mapped them. It exists for the
        // NEW/CHANGED episodes that just came out of `toEpisode` (kernel projects
        // no chapters), preserving Swift-side AI chapters across a refresh.
        // M4 deleted the preserved-state merge for transcriptState, AI inbox
        // triage decisions, and the RAG metadata-index flag: all three now ride
        // the Rust projection via the capability-report model (D7) and are
        // derived in `toEpisode`. ad_segments were already projection-only.
        //
        // Chapters remain the sole exception: there is no Rust action to
        // RECEIVE AI-generated chapters in this milestone — `setEpisodeChapters`
        // mutates Swift state only (no kernel dispatch), so chapters can't
        // round-trip. Until the M5.5 chapter-persistence write path lands
        // (a `SetChapters` action + store side-map + projection, mirroring
        // ad_segments), we keep the prior Swift chapters when Rust projects
        // none so AI chapters don't flash empty on a feed-refresh pass.
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
        let projectedEpisodes = episodes

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
        }

        // After the batch flushes: the widget path reads only `snapshot` and
        // `library` (never the episode-projection caches), so running it here —
        // once the single deferred recompute has already landed — is correct
        // and keeps it from being counted as another batched mutation.
        onNowPlayingSnapshot?(snapshot, library)
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
        performMutationBatch {
            state = next
            mergeResolvedProfiles(identity.resolvedProfiles)
        }
        onNowPlayingSnapshot?(snapshot, library)
    }

    /// Project the snapshot-derived settings + last-played episode onto a
    /// working `AppState` copy. Shared verbatim by the full projection and the
    /// snapshot-only fast path so the two can never drift.
    private func projectSnapshotDerivedState(
        into next: inout AppState, snapshot: PodcastUpdate?
    ) {
        // ── Settings ─────────────────────────────────────────────────────
        let ks = snapshot?.settings ?? SettingsSnapshot()
        // OR: preserve Swift-persisted `true` until Rust learns about it
        // via the `update_settings` dispatch that fires on the same change.
        // Without this, a first launch after a code update would reset the
        // onboarding gate because Rust hasn't received the flag yet.
        next.settings.hasCompletedOnboarding = ks.hasCompletedOnboarding || state.settings.hasCompletedOnboarding
        next.settings.autoSkipAds = ks.autoSkipAdsEnabled
        next.settings.autoPlayNext = ks.autoPlayNext
        next.settings.autoMarkPlayedAtEnd = ks.autoMarkPlayedAtEnd
        if let doubleTap = HeadphoneGestureAction(rawValue: ks.headphoneDoubleTapAction) {
            next.settings.headphoneDoubleTapAction = doubleTap
        }
        if let tripleTap = HeadphoneGestureAction(rawValue: ks.headphoneTripleTapAction) {
            next.settings.headphoneTripleTapAction = tripleTap
        }
        next.settings.skipForwardSeconds = Int(ks.skipForwardSecs)
        next.settings.skipBackwardSeconds = Int(ks.skipBackwardSecs)

        // ── Last-played episode ───────────────────────────────────────────
        if let episodeIdStr = snapshot?.nowPlaying?.episodeId,
           let uuid = UUID(uuidString: episodeIdStr) {
            next.lastPlayedEpisodeID = uuid
        }
    }

    /// Fold the kernel's resolved-profiles map into `nostrProfileCache`. Each
    /// entry becomes a minimal `NostrProfileMetadata` (display → displayName,
    /// pictureUrl → picture) so agent-conversation views resolve a name and
    /// avatar without a Swift-side relay round-trip. Idempotent via the
    /// `setNostrProfile` createdAt guard.
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
