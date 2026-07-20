import Foundation
import Pod0Core

extension PlaybackState {
    func setEpisode(_ newEpisode: Episode) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.select(
            episodeId: EpisodeId(uuid: newEpisode.id),
            segment: nil,
            label: nil
        ))
    }

    func togglePlayPause() {
        isPlaying ? pause() : play()
    }

    func play() {
        guard let sharedCore else { return }
        pendingPlaySignal = true
        Haptics.medium()
        sharedCore.dispatchPlayback(.play)
    }

    func pause() {
        guard let sharedCore else { return }
        Haptics.soft()
        sharedCore.dispatchPlayback(.pause)
    }

    func seek(to time: TimeInterval) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.seek(
            positionMilliseconds: Self.coreMilliseconds(time)
        ))
        Haptics.selection()
    }

    func seekSnapping(to time: TimeInterval) {
        seek(to: time)
    }

    func skipBackward(_ seconds: TimeInterval? = nil) {
        seek(to: max(0, currentTime - (seconds ?? TimeInterval(skipBackwardSeconds))))
    }

    func skipForward(_ seconds: TimeInterval? = nil) {
        seek(to: min(duration, currentTime + (seconds ?? TimeInterval(skipForwardSeconds))))
    }

    func setRate(_ newRate: PlaybackRate) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.setRate(rate: PlaybackRatePermille(
            value: UInt16(clamping: Int((newRate.rawValue * 1_000).rounded()))
        )))
        Haptics.selection()
    }

    var skipForwardSeconds: Int { Int(engine.skipForwardSeconds) }
    var skipBackwardSeconds: Int { Int(engine.skipBackwardSeconds) }

    func applyPreferences(from settings: Settings) {
        engine.skipForwardSeconds = Double(max(1, settings.skipForwardSeconds))
        engine.skipBackwardSeconds = Double(max(1, settings.skipBackwardSeconds))
        sharedCore?.dispatchPlayback(.setPreferences(
            autoMarkPlayedAtNaturalEnd: settings.autoMarkPlayedAtEnd,
            autoPlayNext: settings.autoPlayNext,
            // #104 flips this to the user setting in the same change that
            // activates Rust chapter authority and removes the Swift policy.
            autoSkipAds: false
        ))
        autoSkipAdsEnabled = settings.autoSkipAds
        headphoneDoubleTapAction = settings.headphoneDoubleTapAction
        headphoneTripleTapAction = settings.headphoneTripleTapAction
    }

    func setSleepTimer(_ timer: PlaybackSleepTimer) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.setSleepTimer(mode: timer.coreValue))
        Haptics.selection()
    }
}
