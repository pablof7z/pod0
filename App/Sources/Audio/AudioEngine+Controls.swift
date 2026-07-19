import AVFoundation
import Foundation
import os.log

@MainActor
extension AudioEngine {
    /// Activates the audio session lazily so app launch does not preempt audio.
    func play() {
        guard episode != nil else { return }
        do {
            try AudioSessionCoordinator.shared.activate(.podcastPlayback)
        } catch {
            let engineError = EngineError(error)
            logger.error("Audio session activation failed: \(engineError.description, privacy: .public)")
            setState(.failed(engineError))
            return
        }
        applyEffectiveVolume()
        player.playImmediately(atRate: Float(rate))
        if state != .buffering { setState(.playing) }
        publishNowPlaying()
    }

    func pause() {
        player.pause()
        setState(.paused)
        publishNowPlaying()
    }

    func toggle() {
        switch state {
        case .playing, .buffering: pause()
        case .paused, .idle: play()
        case .loading, .failed: break
        }
    }

    /// Updates the observable playhead before AVPlayer completes the seek so
    /// native presentation and core observations cannot persist stale time.
    func seek(to seconds: TimeInterval) {
        let target = max(0, min(seconds, duration > 0 ? duration : seconds))
        if duration <= 0 || target < duration - 5 {
            didReachNaturalEnd = false
        }
        setCurrentTime(target)
        let time = CMTime(seconds: target, preferredTimescale: 600)
        player.seek(to: time, toleranceBefore: .zero, toleranceAfter: .zero) { [weak self] _ in
            Task { @MainActor in
                self?.publishNowPlayingElapsed()
            }
        }
    }

    func skip(forward seconds: TimeInterval? = nil) {
        seek(to: currentTime + (seconds ?? skipForwardSeconds))
    }

    func skip(back seconds: TimeInterval? = nil) {
        seek(to: currentTime - (seconds ?? skipBackwardSeconds))
    }

    func setRate(_ newRate: Double) {
        let clamped = min(max(newRate, 0.5), 3.0)
        setPlaybackRate(clamped)
        if player.timeControlStatus == .playing {
            player.rate = Float(clamped)
        }
        publishNowPlaying()
    }

    func setSleepTimer(_ mode: SleepTimer.Mode) {
        sleepTimer.set(mode)
        onHostStateChanged()
    }

    func setNowPlayingCallbacks(_ callbacks: NowPlayingCenter.Callbacks) {
        nowPlaying.setCallbacks(callbacks)
    }
}
