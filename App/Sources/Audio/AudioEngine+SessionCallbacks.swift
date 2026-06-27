import Foundation

// MARK: - AVAudioSession callbacks

@MainActor
extension AudioEngine {

    /// Wire the coordinator's interruption / route-change callbacks to this
    /// engine. Called once from `init()`.
    ///
    /// Interruption began: record whether the engine was playing so we know
    /// whether to auto-resume when the interruption ends. By the time `.ended`
    /// fires, AVPlayer KVO has already pushed state to `.paused`, so we capture
    /// intent at `.began` time.
    ///
    /// Interruption end (`shouldResume`): the OS signals that the audio session
    /// can be reactivated (e.g. after a phone call ends). Resume playback only
    /// when the engine was already playing before the interruption — never
    /// autoplay from an idle or user-paused state.
    ///
    /// Route change (output lost): headphones / AirPods were disconnected. iOS
    /// silences `AVPlayer` automatically, but the engine's `state` stays
    /// `.playing`. Syncing to `.paused` keeps the Now-Playing controls and the
    /// in-app UI consistent with the real player state.
    func configureSessionCallbacks() {
        let coordinator = AudioSessionCoordinator.shared

        coordinator.onInterruptionBegan = { [weak self] in
            guard let self else { return }
            self.wasPlayingBeforeInterruption =
                (self.state == .playing || self.state == .buffering)
        }

        coordinator.onInterruptionEnd = { [weak self] in
            guard let self else { return }
            // Only resume if playback was active when the interruption started.
            // Avoid auto-starting from an idle or user-paused state.
            guard self.wasPlayingBeforeInterruption else { return }
            self.wasPlayingBeforeInterruption = false
            self.play()
        }

        coordinator.onRouteChangeOutputLost = { [weak self] in
            guard let self else { return }
            guard self.state == .playing || self.state == .buffering else { return }
            self.pause()
        }
    }
}
