import AVFoundation
import Foundation

// MARK: - Observers

/// `AVPlayer` / `AVPlayerItem` KVO + periodic time observer wiring. Split out
/// of `AudioEngine.swift` to honor the soft 300-line limit (AGENTS.md).
@MainActor
extension AudioEngine {

    // MARK: - Time observer

    func installTimeObserver() {
        if let token = timeObserverToken {
            player.removeTimeObserver(token)
            timeObserverToken = nil
        }
        // 0.5 s ticks — smooth enough for a scrubber, light on CPU.
        let interval = CMTime(seconds: 0.5, preferredTimescale: 600)
        timeObserverToken = player.addPeriodicTimeObserver(
            forInterval: interval,
            queue: .main
        ) { [weak self] time in
            MainActor.assumeIsolated {
                guard let self else { return }
                let seconds = time.seconds.isFinite ? time.seconds : 0
                self.setCurrentTime(seconds)
                self.publishNowPlayingElapsed()
                self.republishIfChapterChanged()
            }
        }
    }

    /// When the active chapter changes mid-playback, the lightweight
    /// `publishNowPlayingElapsed` path won't refresh the lock-screen album
    /// line — only the elapsed/rate. Detect the crossing here and call the
    /// full `publishNowPlaying` so the new chapter title appears.
    func republishIfChapterChanged() {
        guard let episode else { return }
        let current = resolveActiveChapterTitle(episode, currentTime)
        if current != lastPublishedChapterTitle {
            publishNowPlaying()
        }
    }

    // MARK: - Item observers

    func installItemObservers(for item: AVPlayerItem) {
        statusObservation = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            Task { @MainActor in self?.handleItemStatusChange(item) }
        }
        bufferEmptyObservation = item.observe(\.isPlaybackBufferEmpty, options: [.new]) { [weak self] item, _ in
            Task { @MainActor in
                guard let self, item.isPlaybackBufferEmpty else { return }
                if self.state == .playing { self.setState(.buffering) }
            }
        }
        bufferLikelyToKeepUpObservation = item.observe(\.isPlaybackLikelyToKeepUp, options: [.new]) { [weak self] item, _ in
            Task { @MainActor in
                guard let self, item.isPlaybackLikelyToKeepUp else { return }
                if self.state == .buffering { self.setState(.playing) }
            }
        }
        timeControlObservation = player.observe(\.timeControlStatus, options: [.new]) { [weak self] player, _ in
            Task { @MainActor in self?.handleTimeControlChange(player.timeControlStatus) }
        }
        endObserver = NotificationCenter.default.addObserver(
            forName: AVPlayerItem.didPlayToEndTimeNotification,
            object: item,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.handleEndOfItem() }
        }
    }

    func teardownItemObservers() {
        statusObservation?.invalidate(); statusObservation = nil
        bufferEmptyObservation?.invalidate(); bufferEmptyObservation = nil
        bufferLikelyToKeepUpObservation?.invalidate(); bufferLikelyToKeepUpObservation = nil
        timeControlObservation?.invalidate(); timeControlObservation = nil
        if let endObserver {
            NotificationCenter.default.removeObserver(endObserver)
            self.endObserver = nil
        }
    }

    // MARK: - Handlers

    func handleItemStatusChange(_ item: AVPlayerItem) {
        switch item.status {
        case .readyToPlay:
            // Prefer the asset's duration over the feed-supplied one if known.
            let assetDuration = item.duration.seconds
            if assetDuration.isFinite, assetDuration > 0 {
                setDuration(assetDuration)
            }
            // Coming out of `.loading` to `.paused` — caller must `play()` to start.
            if case .loading = state { setState(.paused) }
            publishNowPlaying()
        case .failed:
            let msg = item.error?.localizedDescription ?? "Playback failed"
            setState(.failed(EngineError(msg)))
        default:
            break
        }
    }

    func handleTimeControlChange(_ status: AVPlayer.TimeControlStatus) {
        switch status {
        case .playing:
            if state != .playing { setState(.playing) }
        case .paused:
            // Don't downgrade `.failed` or `.idle` to `.paused`.
            if case .playing = state { setState(.paused) }
            else if case .buffering = state { setState(.paused) }
        case .waitingToPlayAtSpecifiedRate:
            if state == .playing { setState(.buffering) }
        @unknown default:
            break
        }
    }

    func handleEndOfItem() {
        setCurrentTime(duration)
        didReachNaturalEnd = true
        publishNowPlayingElapsed()
        // Sleep timer "end of episode" mode wins if armed.
        if sleepTimer.shouldStopAtEpisodeEnd() {
            setState(.paused)
            return
        }
        setState(.paused)
        // Lane 2 / Lane 4 will hook autoplay-next here; the engine stays neutral.
    }
}
