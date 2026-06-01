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
            while !Task.isCancelled {
                // Apply current state FIRST, then arm the observation for the
                // next change. This eliminates the race where the kernel snapshot
                // advances between `attachKernel` returning and this Task's first
                // iteration — without this, `withObservationTracking` arms on an
                // already-final value and never fires, leaving the UI empty.
                self?.applyKernelState(
                    library: kernel.library,
                    snapshot: kernel.podcastSnapshot,
                    identity: kernel.kernelIdentity)
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

    /// Dedicated logger for the backfill path. `AppStateStore.logger` is
    /// `private` to its defining file, so this extension declares its own.
    private static let backfillLogger = Logger.app("SyntheticEpisodeBackfill")

    /// `UserDefaults` key gating the one-shot pre-#215 synthetic-episode
    /// backfill. Set `true` only after a backfill pass completes, so a pass
    /// interrupted by a crash or early termination retries on the next launch
    /// rather than silently leaving episodes stranded in the Swift-only store.
    private static let syntheticBackfillDoneKey =
        "synthetic_episode_backfill_v1_done"

    /// One-shot migration: re-register agent-generated episodes that predate
    /// PR #215 (`kernelRegisterSyntheticEpisode`) into the Rust kernel store.
    ///
    /// Before #215, `AgentTTSComposer` wrote produced episodes into the Swift
    /// render store only. The kernel projection is now the source of truth and
    /// `applyKernelState` does a full-replace of `state.episodes`, so those
    /// legacy episodes would vanish on the first projection tick after a user
    /// updates to a #215+ build. This walks the still-persisted Swift state
    /// (captured before that first tick — see the call site in `attachKernel`),
    /// seeds the kernel's default "Agent Generated" podcast row, and registers
    /// each surviving episode so it rides the next projection back into the UI
    /// and becomes resolvable by `publish_episode`.
    ///
    /// Scope is intentionally narrow: only the default Agent Generated show,
    /// identified by its stable `sentinelFeedURL`. Other `.synthetic` shows are
    /// agent-OWNED podcasts whose kernel rows are seeded through their own
    /// create/publish lifecycle; a blind re-register there would orphan
    /// episodes under a missing row.
    ///
    /// The legacy show is matched by `sentinelFeedURL`, NOT by
    /// `defaultPodcastID`: that stable id was introduced in PR #215, but the
    /// pre-#215 `ensurePodcastID` created the synthetic `Podcast` row without an
    /// explicit id, so the initializer defaulted it to a random `UUID()`.
    /// Pre-#215 episodes are therefore parented to that random id. We resolve
    /// the legacy id(s) from the still-persisted `state.podcasts`, collect their
    /// episodes, and re-register them under the stable `defaultPodcastID` — the
    /// kernel row seeded just below — consolidating the show under one identity.
    func backfillSyntheticEpisodes() {
        let defaults = UserDefaults.standard
        guard !defaults.bool(forKey: Self.syntheticBackfillDoneKey) else { return }

        let defaultPodcastID = AgentGeneratedPodcastService.defaultPodcastID
        // Resolve the legacy (random-id) Agent Generated row(s) by the stable
        // sentinel feed URL, plus the stable id itself for episodes already
        // produced by a #215+ build that ran before this backfill shipped.
        let sentinel = AgentGeneratedPodcastService.sentinelFeedURL
        let legacyPodcastIDs = Set(
            state.podcasts
                .filter { $0.feedURL == sentinel || $0.id == defaultPodcastID }
                .map(\.id)
        )
        let legacyEpisodes = state.episodes.filter { legacyPodcastIDs.contains($0.podcastID) }
        guard !legacyEpisodes.isEmpty else {
            // Nothing to migrate (fresh install, or every episode already
            // produced by a #215+ build). Mark done so we never walk again.
            defaults.set(true, forKey: Self.syntheticBackfillDoneKey)
            return
        }

        // Seed the kernel's default synthetic podcast row first. Idempotent:
        // `create_synthetic_podcast` keys on the stable id, and the Swift
        // `upsertPodcast` mirror is insert-only by id. Without the row the
        // kernel would have nowhere to attach the registered episodes.
        _ = AgentGeneratedPodcastService.ensurePodcastID(in: self)

        var registered = 0
        for episode in legacyEpisodes {
            // Resolve the on-disk audio path: prefer the downloaded local file,
            // fall back to a `file://` enclosure URL. A synthetic episode is
            // produced as a downloaded m4a, so one of these is normally set.
            let downloadedPath: String? = {
                if case let .downloaded(localFileURL, _) = episode.downloadState {
                    return localFileURL.path
                }
                return nil
            }()
            let audioPath = downloadedPath
                ?? (episode.enclosureURL.isFileURL ? episode.enclosureURL.path : nil)
            guard let path = audioPath,
                  FileManager.default.fileExists(atPath: path) else {
                // Audio file is gone — registering would resurrect a dead row
                // that can never play. Skip it.
                continue
            }

            let chapterWire = (episode.chapters ?? []).map(AgentTTSComposer.chapterWire)
            let transcript = TranscriptStore.shared.load(episodeID: episode.id)?
                .segments.map(\.text).joined(separator: " ").nilIfEmpty

            kernelRegisterSyntheticEpisode(
                podcastId: defaultPodcastID.uuidString,
                episodeId: episode.id.uuidString,
                title: episode.title,
                audioPath: path,
                durationSecs: episode.duration,
                chapters: chapterWire,
                transcript: transcript
            )
            registered += 1
        }

        Self.backfillLogger.info(
            "Synthetic-episode backfill: registered \(registered, privacy: .public) of \(legacyEpisodes.count, privacy: .public) legacy agent episode(s) into the kernel"
        )
        // Flag set ONLY after the loop completes, so an interrupted pass retries.
        defaults.set(true, forKey: Self.syntheticBackfillDoneKey)
    }

    /// Project the current kernel state into `AppState`.
    /// Takes `library` and `snapshot` separately because `KernelModel` gates
    /// them on different content hashes. `identity` carries the kernel's
    /// resolved-profiles map, merged into `nostrProfileCache` after the main
    /// projection lands.
    private func applyKernelState(
        library: [PodcastSummary],
        snapshot: PodcastUpdate?,
        identity: KernelIdentityProjection
    ) {
        // Count is computed allocation-free (reduce, not flatMap) so the
        // signpost label adds no O(N) array copy to this hot path — the
        // os_signpost API defers FORMATTING, not argument evaluation.
        let applyInterval = signposter.beginInterval(
            "applyKernelState", "episodes=\(library.reduce(0) { $0 + $1.episodes.count })")
        defer { signposter.endInterval("applyKernelState", applyInterval) }

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

        // ── Episodes ──────────────────────────────────────────────────────
        var episodes: [Episode] = []
        for summary in library {
            for ep in summary.episodes {
                if let episode = ep.toEpisode(podcastIdString: summary.id) {
                    episodes.append(episode)
                }
            }
        }
        // Also include episodes from the active queue (snapshot may lag library
        // if only library changed, but queue episodes still need to resolve).
        for ep in snapshot?.queue ?? [] {
            let podcastIdString = ep.podcastId ?? Podcast.unknownID.uuidString
            if let episode = ep.toEpisode(podcastIdString: podcastIdString),
               !episodes.contains(where: { $0.id == episode.id }) {
                episodes.append(episode)
            }
        }

        // ── Chapters fallback (last preserved-state field) ───────────────
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
        let priorByID = Dictionary(
            state.episodes.map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first }
        )
        for idx in episodes.indices {
            guard let prior = priorByID[episodes[idx].id] else { continue }
            if episodes[idx].chapters?.isEmpty != false {
                episodes[idx].chapters = prior.chapters
            }
        }
        next.episodes = episodes

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

// MARK: - EpisodeSummary → Episode mapping

private extension EpisodeSummary {
    func toEpisode(podcastIdString: String) -> Episode? {
        guard let episodeUUID = UUID(uuidString: id),
              let podcastUUID = UUID(uuidString: podcastIdString)
        else { return nil }

        let pubDate: Date = publishedAt.map { Date(timeIntervalSince1970: Double($0)) } ?? Date.distantPast

        // For downloaded episodes, use the local file URL. For streaming
        // episodes, use the RSS enclosure URL projected from Rust so the
        // host player can start without a Rust round-trip.
        let enclosureURL: URL = downloadPath.flatMap { URL(fileURLWithPath: $0) }
            ?? enclosureUrl.flatMap { URL(string: $0) }
            ?? URL(string: "https://placeholder.invalid/\(id)")!

        let downloadState: DownloadState
        if let path = downloadPath {
            let fileURL = URL(fileURLWithPath: path)
            // Size is cached by the Rust kernel at download-completion time
            // (`EpisodeSummary.file_size_bytes`), so we avoid a synchronous
            // `URL.resourceValues(.fileSizeKey)` stat on the main actor for
            // every downloaded episode on every projection tick.
            let byteCount: Int64 = fileSizeBytes
            downloadState = .downloaded(localFileURL: fileURL, byteCount: byteCount)
        } else {
            downloadState = .notDownloaded
        }

        let projectedChapters: [Episode.Chapter]? = chapters.flatMap {
            $0.isEmpty ? nil : $0.map(\.toChapter)
        }
        let projectedAdSegments: [Episode.AdSegment]? = adSegments.isEmpty ? nil : adSegments.compactMap { seg in
            guard let uuid = UUID(uuidString: seg.id) else { return nil }
            let kind = Episode.AdKind(rawValue: seg.kind) ?? .midroll
            return Episode.AdSegment(id: uuid, start: seg.startSecs, end: seg.endSecs, kind: kind)
        }
        // Derive transcriptState entirely from the Rust projection (M4 / D7).
        //   1. A non-empty stored `transcript` ⇒ `.ready`. It came from either
        //      iOS STT (kernelTranscriptReport) or a publisher fetch; we can't
        //      distinguish the source from Rust alone, so use `.publisher` as
        //      the conservative default (the precise source lives on the iOS
        //      TranscriptStore for the badge).
        //   2. Otherwise honour the transient status iOS reported via
        //      `set_episode_transcript_status` (queued / fetching publisher /
        //      transcribing / failed). The progress arg is always 0 — the real
        //      pipeline never streams a percentage (it sets `.transcribing(0)`
        //      once before the provider call), so no progress round-trips.
        //   3. No transcript and no override ⇒ `.none`.
        let derivedTranscriptState: TranscriptState? = {
            if let text = transcript, !text.isEmpty {
                return .ready(source: .publisher)
            }
            switch transcriptStatus {
            case "queued": return .queued
            case "fetching_publisher": return .fetchingPublisher
            case "transcribing": return .transcribing(progress: 0)
            case "failed":
                return .failed(message: transcriptStatusMessage ?? "Transcription didn't finish.")
            default: return nil
            }
        }()

        return Episode(
            id: episodeUUID,
            podcastID: podcastUUID,
            guid: id,
            title: title,
            description: description ?? "",
            pubDate: pubDate,
            duration: durationSecs,
            enclosureURL: enclosureURL,
            imageURL: artworkUrl.flatMap { URL(string: $0) },
            chapters: projectedChapters,
            publisherTranscriptURL: transcriptUrl.flatMap { URL(string: $0) },
            playbackPosition: playbackPositionSecs ?? 0,
            played: played,
            isStarred: starred,
            downloadState: downloadState,
            transcriptState: derivedTranscriptState ?? .none,
            adSegments: projectedAdSegments,
            // M4 / D7: all three derive from the Rust projection now — no
            // preserved-state merge. `triageDecision` parses the rawValue
            // ("inbox" / "archived"); an absent / unrecognised value ⇒ nil
            // (untriaged).
            triageDecision: triageDecision.flatMap { TriageDecision(rawValue: $0) },
            triageRationale: triageRationale,
            triageIsHero: triageIsHero,
            metadataIndexed: metadataIndexed,
            // #45: AI-generated category labels. Projection-only — the
            // kernel owns them, so they ride the snapshot straight onto the
            // domain model with no preserved-state merge.
            aiCategories: aiCategories,
            // AI episode summary. Projection-only — produced by the kernel
            // `summarize_episode` pass and carried straight onto the domain
            // model so `store.episode(id:).summary` reflects it.
            summary: summary
        )
    }
}

// MARK: - ChapterSummary → Episode.Chapter

private extension ChapterSummary {
    var toChapter: Episode.Chapter {
        Episode.Chapter(
            startTime: startSecs,
            endTime: endSecs,
            title: title,
            imageURL: imageUrl.flatMap { URL(string: $0) },
            linkURL: url.flatMap { URL(string: $0) },
            isAIGenerated: isAiGenerated,
            sourceEpisodeID: sourceEpisodeId
        )
    }
}
