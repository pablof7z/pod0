import Foundation
import Pod0Core

extension PlaybackState {
    func attachSharedCore(_ client: SharedLibraryClient) {
        guard sharedCore !== client else { return }
        sharedCore = client
        engine.onPresentationTimeChanged = { [weak self] time in
            self?.handleSharedPresentationTime(time)
        }
    }

    func applySharedPlayback(
        _ projection: PlaybackProjection,
        stateRevision: UInt64,
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
                pendingResumeSignal = nil
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
        if isNewEpisode, current.durableResumePositionMilliseconds > 0 {
            pendingResumeSignal = (
                resolved.id,
                Double(current.durableResumePositionMilliseconds) / 1_000
            )
        }
        if let pending = pendingResumeSignal,
           pending.episodeID == resolved.id,
           engine.episode?.id == resolved.id,
           abs(engine.currentTime - pending.position) <= 1 {
            recordResumeAttempt(expectedPosition: pending.position)
            pendingResumeSignal = nil
        }
        if current.meaningfulListeningReached {
            recordMeaningfulListening(
                episodeID: resolved.id,
                domainRevision: stateRevision
            )
        }
        if pendingPlaySignal {
            switch current.policyState {
            case .playing:
                recordPlaybackSignal(name: .playStarted, outcome: .succeeded)
                pendingPlaySignal = false
            case .failed:
                recordPlaybackSignal(name: .playStarted, outcome: .failed)
                pendingPlaySignal = false
            default:
                break
            }
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
