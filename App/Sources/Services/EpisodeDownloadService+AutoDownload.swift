import Foundation

// MARK: - AutoDownloadPolicy

extension EpisodeDownloadService {

    /// Evaluates the per-podcast `AutoDownloadPolicy` against a batch of
    /// episode IDs that were just inserted by `upsertEpisodes`. Queues the
    /// matching ones via `download(episodeID:)`.
    ///
    /// - Parameter newEpisodeIDs: episodes inserted in publish-date order
    ///   (newest first is fine — we sort defensively).
    func evaluateAutoDownload(forPodcast podcastID: UUID, newEpisodeIDs: [UUID]) {
        guard !newEpisodeIDs.isEmpty,
              let store = appStore,
              store.podcast(id: podcastID) != nil else { return }
        // Honour any per-category auto-download override before falling back
        // to the per-podcast policy. `effectiveAutoDownload` resolves to the
        // subscription's `autoDownload` when no category settings apply.
        let policy = store.effectiveAutoDownload(forPodcast: podcastID)
        if case .off = policy.mode { return }
        // Resolve each ID to an Episode and sort by pubDate desc.
        let episodes: [Episode] = newEpisodeIDs
            .compactMap { store.episode(id: $0) }
            .sorted { $0.pubDate > $1.pubDate }
        let targets: [Episode]
        switch policy.mode {
        case .off:
            return
        case .latestN(let n):
            targets = Array(episodes.prefix(max(0, n)))
        case .allNew:
            targets = episodes
        }
        if policy.wifiOnly, !isOnWiFi {
            for episode in targets {
                queueAutoDownload(episode)
            }
            logger.notice(
                "auto-download queued for \(podcastID, privacy: .public) — Wi-Fi unavailable"
            )
            return
        }
        for episode in targets {
            // Only queue ones we don't already have on disk / in flight.
            switch episode.downloadState {
            case .downloaded, .downloading:
                continue
            default:
                download(episodeID: episode.id)
            }
        }
    }

    func resumeQueuedDownloadsIfPossible() {
        guard isOnWiFi, let store = appStore else { return }
        let queued = store.state.episodes.filter {
            if case .queued = $0.downloadState { return true }
            return false
        }
        for episode in queued {
            let policy = store.effectiveAutoDownload(forPodcast: episode.podcastID)
            if case .off = policy.mode { continue }
            download(episodeID: episode.id)
        }
    }

    /// Ensure a background download exists for `episodeID`.
    ///
    /// Used by the playback boundary: when the user starts streaming an
    /// episode whose enclosure isn't on disk, this kicks off the same
    /// download → transcription → chapters pipeline that explicit
    /// "Download" taps use, without blocking playback.
    ///
    /// No-op when the episode is already `.downloading`, `.queued`, or
    /// `.downloaded` — restarting in-flight work would spam the URLSession
    /// queue and clobber resume data. `.notDownloaded` and `.failed` are
    /// the only cases that re-enqueue.
    ///
    /// Honours the Wi-Fi guard the same way `evaluateAutoDownload` does:
    /// off-Wi-Fi, the episode is marked `.queued` so
    /// `resumeQueuedDownloadsIfPossible` picks it up when Wi-Fi returns.
    /// On Wi-Fi, the request starts immediately. The Wi-Fi policy comes
    /// from the per-podcast auto-download policy so the playback path
    /// respects the same user preference as auto-download.
    func ensureDownloadEnqueued(episodeID: UUID) {
        guard let store = appStore,
              let episode = store.episode(id: episodeID) else { return }
        switch episode.downloadState {
        case .downloading, .queued, .downloaded:
            return
        case .notDownloaded, .failed:
            break
        }
        let policy = store.effectiveAutoDownload(forPodcast: episode.podcastID)
        if policy.wifiOnly, !isOnWiFi {
            queueAutoDownload(episode)
            logger.notice(
                "playback-triggered download queued for \(episodeID, privacy: .public) — Wi-Fi unavailable"
            )
            return
        }
        download(episodeID: episodeID)
    }

    private func queueAutoDownload(_ episode: Episode) {
        guard let store = appStore else { return }
        switch episode.downloadState {
        case .downloaded, .downloading, .queued:
            return
        case .notDownloaded, .failed:
            store.setEpisodeDownloadState(episode.id, state: .queued)
        }
    }
}
