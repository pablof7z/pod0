import Foundation
import Pod0Core

// MARK: - Audio callbacks

extension PlaybackState {

    /// Keep system-originated commands on the same typed Rust boundary as UI taps.
    func configureAudioEngineCallbacks() {
        var callbacks = NowPlayingCenter.Callbacks()
        callbacks.play = { [weak self] in self?.play() }
        callbacks.pause = { [weak self] in self?.pause() }
        callbacks.toggle = { [weak self] in self?.togglePlayPause() }
        callbacks.skipForward = { [weak self] in self?.skipForward() }
        callbacks.skipBackward = { [weak self] in self?.skipBackward() }
        callbacks.seek = { [weak self] time in self?.seek(to: time) }
        callbacks.changeRate = { [weak self] rate in self?.setRate(rate) }
        // AirPods double/triple-tap (or any source emitting next/previous
        // track) routes through the user-configured action.
        callbacks.nextTrack = { [weak self] in
            guard let self else { return }
            self.performHeadphoneGesture(self.headphoneDoubleTapAction)
        }
        callbacks.previousTrack = { [weak self] in
            guard let self else { return }
            self.performHeadphoneGesture(self.headphoneTripleTapAction)
        }
        engine.setNowPlayingCallbacks(callbacks)

        engine.onSleepTimerFire = { [weak self] in
            self?.sharedCore?.dispatchPlayback(.nativeTimerFired)
        }
        engine.onFailure = { [weak self] failure in
            self?.recordPlaybackSignal(
                name: .playbackError,
                outcome: .failed,
                errorClass: failure.code
            )
        }
    }

    func setRate(_ newRate: Double) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.setRate(rate: PlaybackRatePermille(
            value: UInt16(clamping: Int((newRate * 1_000).rounded()))
        )))
    }
}
