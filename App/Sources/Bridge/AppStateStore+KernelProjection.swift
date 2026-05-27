import Foundation
import Observation

// MARK: - KernelModel → AppState projection
//
// Observes `KernelModel.podcastSnapshot` on every tick and updates
// `AppStateStore.state` so all existing views read real Rust-backed data
// without any changes to the view layer.
//
// ID stability: Rust emits UUIDv5 strings for both PodcastId and EpisodeId
// (derived from feedURL|guid). `UUID(uuidString:)` parses them reliably,
// preserving foreign-key relationships across the projection.

extension AppStateStore {

    /// Call once from `AppMain` after both `store` and `kernelModel` exist.
    /// Polls the kernel snapshot at 500ms intervals (matching the kernel's
    /// own poll cadence) and projects changes into `AppState`.
    @MainActor
    func attachKernel(_ kernel: KernelModel) {
        snapshotObservationTask?.cancel()
        snapshotObservationTask = Task { @MainActor [weak self, weak kernel] in
            guard let self, let kernel else { return }
            var lastRev = 0
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(500))
                guard !Task.isCancelled else { break }
                guard let snap = kernel.podcastSnapshot, snap.rev != lastRev else { continue }
                lastRev = snap.rev
                self.applyKernelSnapshot(snap)
            }
        }
    }

    /// Project a full `PodcastUpdate` snapshot into `AppState`.
    private func applyKernelSnapshot(_ snap: PodcastUpdate) {
        var next = state

        // ── Podcasts + subscriptions ──────────────────────────────────────
        var podcasts: [Podcast] = []
        var subscriptions: [PodcastSubscription] = []

        for summary in snap.library {
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
        for summary in snap.library {
            for ep in summary.episodes {
                if let episode = ep.toEpisode(podcastIdString: summary.id) {
                    episodes.append(episode)
                }
            }
        }
        // Also include episodes from the active queue.
        for ep in snap.queue {
            let podcastIdString = ep.podcastId ?? Podcast.unknownID.uuidString
            if let episode = ep.toEpisode(podcastIdString: podcastIdString),
               !episodes.contains(where: { $0.id == episode.id }) {
                episodes.append(episode)
            }
        }
        next.episodes = episodes

        // ── Settings ─────────────────────────────────────────────────────
        let ks = snap.settings
        next.settings.hasCompletedOnboarding = ks.hasCompletedOnboarding
        next.settings.autoSkipAds = ks.autoSkipAdsEnabled
        next.settings.skipForwardSeconds = Int(ks.skipForwardSecs)
        next.settings.skipBackwardSeconds = Int(ks.skipBackwardSecs)

        // ── Last-played episode ───────────────────────────────────────────
        if let episodeIdStr = snap.nowPlaying?.episodeId,
           let uuid = UUID(uuidString: episodeIdStr) {
            next.lastPlayedEpisodeID = uuid
        }

        state = next
    }

    // MARK: - Stored observation task

    var snapshotObservationTask: Task<Void, Never>? {
        get { objc_getAssociatedObject(self, AppStateStore.observationTaskKey) as? Task<Void, Never> }
        set { objc_setAssociatedObject(self, AppStateStore.observationTaskKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }

    private static let observationTaskKey = UnsafeMutableRawPointer.allocate(byteCount: 1, alignment: 1)
}

// MARK: - EpisodeSummary → Episode mapping

private extension EpisodeSummary {
    func toEpisode(podcastIdString: String) -> Episode? {
        guard let episodeUUID = UUID(uuidString: id),
              let podcastUUID = UUID(uuidString: podcastIdString)
        else { return nil }

        let pubDate: Date = publishedAt.map { Date(timeIntervalSince1970: Double($0)) } ?? Date.distantPast

        // For display purposes, use the download path as a file URL when
        // available. Playback will be handled by the Rust kernel (Phase 2);
        // this URL only needs to be non-nil for Episode to be created.
        let enclosureURL: URL = downloadPath.flatMap { URL(fileURLWithPath: $0) as URL? }
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
