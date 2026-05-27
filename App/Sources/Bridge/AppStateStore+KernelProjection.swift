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
        next.episodes = episodes

        // ── Settings ─────────────────────────────────────────────────────
        let ks = snapshot?.settings ?? SettingsSnapshot()
        next.settings.hasCompletedOnboarding = ks.hasCompletedOnboarding
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
            publisherTranscriptURL: transcriptUrl.flatMap { URL(string: $0) },
            playbackPosition: playbackPositionSecs ?? 0,
            played: played,
            isStarred: starred,
            downloadState: downloadState
        )
    }
}
