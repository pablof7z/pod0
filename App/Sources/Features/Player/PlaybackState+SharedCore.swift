import Foundation
import Pod0Core

extension PlaybackState {
    func attachSharedCore(_ client: SharedLibraryClient) {
        guard sharedCore !== client else { return }
        sharedCore = client
        persistenceTask?.cancel()
        persistenceTask = nil
        engine.onPresentationTimeChanged = { [weak self] time in
            self?.handleSharedPresentationTime(time)
        }
    }

    func applySharedPlayback(
        _ projection: PlaybackProjection,
        resolveEpisode: (UUID) -> Episode?
    ) {
        sleepTimer = projection.sleepMode.swiftValue
        queue = projection.queue.compactMap(\.swiftValue)

        guard let current = projection.current,
              let id = current.episodeId.uuid,
              let resolved = resolveEpisode(id)
        else {
            if projection.current == nil {
                episode = nil
                currentSegmentEndTime = nil
            }
            writeNowPlayingSnapshot(force: true)
            return
        }

        let isNewEpisode = episode?.id != resolved.id
        episode = resolved
        adSegments = resolved.adSegments ?? []
        currentSegmentEndTime = current.segment?.endPositionMilliseconds.map {
            Double($0) / 1_000
        }
        if engine.episode?.id == resolved.id {
            engine.refreshMetadata(for: resolved)
        }
        if isNewEpisode, case .notDownloaded = resolved.downloadState {
            onEnsureDownloadEnqueued(resolved.id)
        }
        writeNowPlayingSnapshot(force: true)
    }

    func handleSharedPresentationTime(_ time: TimeInterval) {
        guard sharedCore != nil else { return }
        applyAutoSkipAdsIfNeeded(at: time)
        writeNowPlayingSnapshot(force: false)
    }

    static func coreMilliseconds(_ seconds: TimeInterval) -> UInt64 {
        guard seconds.isFinite, seconds > 0 else { return 0 }
        return UInt64(min(seconds * 1_000, Double(UInt64.max)).rounded())
    }
}
