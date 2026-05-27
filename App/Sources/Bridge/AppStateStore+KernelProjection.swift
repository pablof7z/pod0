import Foundation
import Observation

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
        kernelObservationTask?.cancel()
        // Apply immediately so the first render sees real data even before the
        // first observation-change fires.
        applyKernelState(library: kernel.library, snapshot: kernel.podcastSnapshot)
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
            while !Task.isCancelled {
                // Suspend until either kernel.library or kernel.podcastSnapshot changes.
                // withObservationTracking fires onChange once and returns; we loop
                // to re-arm continuously.
                await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                    withObservationTracking {
                        _ = kernel.library
                        _ = kernel.podcastSnapshot
                    } onChange: {
                        continuation.resume()
                    }
                }
                guard !Task.isCancelled else { break }
                self?.applyKernelState(library: kernel.library, snapshot: kernel.podcastSnapshot)
            }
        }
    }

    /// Project the current kernel state into `AppState`.
    /// Takes `library` and `snapshot` separately because `KernelModel` gates
    /// them on different content hashes.
    private func applyKernelState(library: [PodcastSummary], snapshot: PodcastUpdate?) {
        var next = state

        // ── Podcasts + subscriptions ──────────────────────────────────────
        var podcasts: [Podcast] = []
        var subscriptions: [PodcastSubscription] = []

        for summary in library {
            guard let uuid = UUID(uuidString: summary.id) else { continue }
            let feedURL = summary.feedUrl.flatMap { URL(string: $0) }
            podcasts.append(Podcast(
                id: uuid,
                kind: .rss,
                feedURL: feedURL,
                title: summary.title,
                author: summary.author ?? "",
                imageURL: summary.artworkUrl.flatMap { URL(string: $0) },
                description: summary.description ?? ""
            ))
            let autoDownload: AutoDownloadPolicy = summary.autoDownload
                ? AutoDownloadPolicy(mode: .allNew, wifiOnly: true)
                : AutoDownloadPolicy(mode: .off, wifiOnly: true)
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

        // ── Preserve Swift-only episode state across projection passes ────
        // Rust does not own: transcript state, AI inbox triage decisions,
        // ad segments, RAG metadata index flag, or AI-generated chapters.
        // Without this merge those fields would be silently wiped on every
        // feed refresh (which advances the library hash and re-runs this
        // projection).
        let priorByID = Dictionary(
            state.episodes.map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first }
        )
        for idx in episodes.indices {
            guard let prior = priorByID[episodes[idx].id] else { continue }
            episodes[idx].transcriptState = prior.transcriptState
            episodes[idx].triageDecision = prior.triageDecision
            episodes[idx].triageRationale = prior.triageRationale
            episodes[idx].triageIsHero = prior.triageIsHero
            // Prefer Rust-projected ad segments; fall back to Swift's if Rust has none.
            if episodes[idx].adSegments?.isEmpty != false {
                episodes[idx].adSegments = prior.adSegments
            }
            episodes[idx].metadataIndexed = prior.metadataIndexed
            // Prefer Rust-projected chapters (publisher JSON fetched via
            // `fetch_chapters`). If Rust has none yet, fall back to any
            // chapters stored in Swift. AI-generated chapters are merged on
            // top of publisher chapters so the full set is always visible.
            if episodes[idx].chapters?.isEmpty != false {
                episodes[idx].chapters = prior.chapters
            } else if let aiOnly = prior.chapters?.filter({ $0.isAIGenerated }),
                      !aiOnly.isEmpty {
                episodes[idx].chapters = (episodes[idx].chapters ?? []) + aiOnly
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
        next.settings.skipForwardSeconds = Int(ks.skipForwardSecs)
        next.settings.skipBackwardSeconds = Int(ks.skipBackwardSecs)

        // ── Last-played episode ───────────────────────────────────────────
        if let episodeIdStr = snapshot?.nowPlaying?.episodeId,
           let uuid = UUID(uuidString: episodeIdStr) {
            next.lastPlayedEpisodeID = uuid
        }

        state = next
    }
}

// MARK: - EpisodeSummary → Episode mapping

private extension EpisodeSummary {
    func toEpisode(podcastIdString: String) -> Episode? {
        guard let episodeUUID = UUID(uuidString: id),
              let podcastUUID = UUID(uuidString: podcastIdString)
        else { return nil }

        let pubDate: Date = publishedAt.map { Date(timeIntervalSince1970: Double($0)) } ?? Date.distantPast

        // Use the local file URL when the episode is downloaded; otherwise a
        // stable placeholder. Downloads are triggered through the Rust kernel
        // (dispatch "download"), not directly by iOS code, so the remote URL
        // is not needed in the projection — Rust fetches and reports the path.
        let enclosureURL: URL = downloadPath.flatMap { URL(fileURLWithPath: $0) }
            ?? URL(string: "https://placeholder.invalid/\(id)")!

        let downloadState: DownloadState
        if let path = downloadPath {
            let fileURL = URL(fileURLWithPath: path)
            let byteCount: Int64 = (try? fileURL.resourceValues(forKeys: [.fileSizeKey]).fileSize.map { Int64($0) }) ?? 0
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
            adSegments: projectedAdSegments
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
            isAIGenerated: isAiGenerated
        )
    }
}
