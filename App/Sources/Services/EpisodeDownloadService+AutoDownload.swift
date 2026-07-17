import Foundation

// MARK: - AutoDownloadPolicy

extension EpisodeDownloadService {

    func resumeQueuedDownloadsIfPossible() {
        guard isOnWiFi else { return }
        WorkflowRuntime.shared.dependencyChanged(for: .autoDownload)
    }

    /// Ensure a background download exists for `episodeID`.
    ///
    /// Used by the playback boundary: when the user starts streaming an
    /// episode whose enclosure isn't on disk, this kicks off the same
    /// download → transcription → chapters pipeline that explicit
    /// "Download" taps use, without blocking playback.
    ///
    /// Stable file evidence prevents redundant intent. The canonical job key
    /// absorbs repeated playback requests while work is active.
    func ensureDownloadEnqueued(episodeID: UUID) {
        guard let store = appStore,
              let episode = store.episode(id: episodeID) else { return }
        switch episode.downloadState {
        case .downloaded:
            return
        case .notDownloaded:
            break
        }
        let policy = store.effectiveAutoDownload(forPodcast: episode.podcastID)
        if policy.wifiOnly, !isOnWiFi { return }
        WorkflowRuntime.shared.requestDownload(episodeID: episodeID, origin: .playback)
    }
}
