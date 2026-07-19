import Foundation

extension PlaybackState {
    /// Legacy-only loop retained for tests and disabled stores. The shared
    /// facade receives event-driven AVPlayer observations and owns durable
    /// checkpoint, segment-end, and completion policy in production.
    func startPersistenceLoop() {
        guard sharedCore == nil else { return }
        persistenceTask?.cancel()
        persistenceTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self else { return }
                self.tickLegacyPersistence()
            }
        }
    }

    func tickLegacyPersistence() {
        guard let episode, didFireFinishedFor != episode.id else { return }
        let time = engine.currentTime
        if time > 0 { onPersistPosition(episode.id, time) }

        if let segmentEnd = currentSegmentEndTime, time >= segmentEnd {
            currentSegmentEndTime = nil
            onSegmentFinished()
            return
        }

        applyAutoSkipAdsIfNeeded(at: time)
        writeNowPlayingSnapshot(force: false)
        if engine.didReachNaturalEnd {
            playbackRequested = false
            didFireFinishedFor = episode.id
            if autoMarkPlayedOnFinish {
                onEpisodeFinished(episode.id)
            } else {
                onFlushPositions()
            }
        }
    }
}
