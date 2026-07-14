import AVFoundation
import Foundation
import MediaPlayer
import os.log

// MARK: - NowPlayingCenter

/// Bridges the `AudioEngine` to the system's lock-screen / Control Center /
/// AirPlay surfaces via `MPNowPlayingInfoCenter` and `MPRemoteCommandCenter`.
///
/// Asymmetric skip durations (forward 30s, back 15s) are wired here so the
/// system renders the right glyphs on the lock screen — see baseline-podcast-features.md.
///
/// The center holds weak callbacks back to the engine so it stays decoupled
/// from `AudioEngine`'s concrete type. `AudioEngine` retains the center.
@MainActor
final class NowPlayingCenter {

    // MARK: - Callbacks

    struct Callbacks {
        var play: () -> Void = {}
        var pause: () -> Void = {}
        var toggle: () -> Void = {}
        var skipForward: () -> Void = {}
        var skipBackward: () -> Void = {}
        var seek: (TimeInterval) -> Void = { _ in }
        var changeRate: (Double) -> Void = { _ in }
        /// Fired when iOS delivers `nextTrackCommand` — typically an AirPods
        /// double-tap / double-squeeze. The action is decided upstream
        /// (`PlaybackState`) based on the user's headphone-gesture setting.
        var nextTrack: () -> Void = {}
        /// Fired when iOS delivers `previousTrackCommand` — typically an
        /// AirPods triple-tap / triple-squeeze.
        var previousTrack: () -> Void = {}
    }

    enum RemoteCommand: Equatable {
        case play
        case pause
        case toggle
        case skipForward
        case skipBackward
        case seek(TimeInterval)
        case changeRate(Double)
        case nextTrack
        case previousTrack
    }

    // MARK: - State

    private let logger = Logger.app("NowPlayingCenter")
    private let infoCenter = MPNowPlayingInfoCenter.default()
    private let commandCenter = MPRemoteCommandCenter.shared()
    private var callbacks = Callbacks()
    private(set) var skipForwardSeconds: Double = 30
    private(set) var skipBackwardSeconds: Double = 15
    private var didWireCommands = false

    // MARK: - Init

    init() {}

    // MARK: - Public API

    /// Wire `MPRemoteCommandCenter` handlers. Safe to call repeatedly — the
    /// targets are removed and re-added so callbacks always point at the latest
    /// engine instance.
    func setCallbacks(_ callbacks: Callbacks) {
        self.callbacks = callbacks
        wireCommandsIfNeeded()
    }

    @discardableResult
    func performRemoteCommand(_ command: RemoteCommand) -> MPRemoteCommandHandlerStatus {
        switch command {
        case .play:
            callbacks.play()
        case .pause:
            callbacks.pause()
        case .toggle:
            callbacks.toggle()
        case .skipForward:
            callbacks.skipForward()
        case .skipBackward:
            callbacks.skipBackward()
        case .seek(let time):
            callbacks.seek(time)
        case .changeRate(let rate):
            callbacks.changeRate(rate)
        case .nextTrack:
            callbacks.nextTrack()
        case .previousTrack:
            callbacks.previousTrack()
        }
        return .success
    }

    /// Asymmetric skip — the lock-screen glyphs render based on these values.
    func setSkipIntervals(forward: Double, backward: Double) {
        skipForwardSeconds = forward
        skipBackwardSeconds = backward
        commandCenter.skipForwardCommand.preferredIntervals = [NSNumber(value: forward)]
        commandCenter.skipBackwardCommand.preferredIntervals = [NSNumber(value: backward)]
    }

    /// Push a fresh metadata snapshot. Pass `nil` to clear (e.g. on stop).
    func update(
        title: String?,
        artist: String?,
        albumTitle: String?,
        duration: TimeInterval?,
        elapsed: TimeInterval?,
        rate: Double,
        artwork: MPMediaItemArtwork? = nil
    ) {
        guard let title else {
            infoCenter.nowPlayingInfo = nil
            infoCenter.playbackState = .stopped
            return
        }

        var info: [String: Any] = [
            MPMediaItemPropertyTitle: title,
            MPNowPlayingInfoPropertyMediaType: NSNumber(value: MPNowPlayingInfoMediaType.audio.rawValue),
            MPNowPlayingInfoPropertyPlaybackRate: NSNumber(value: rate),
            MPNowPlayingInfoPropertyDefaultPlaybackRate: NSNumber(value: 1.0)
        ]
        if let artist { info[MPMediaItemPropertyArtist] = artist }
        if let albumTitle { info[MPMediaItemPropertyAlbumTitle] = albumTitle }
        if let duration { info[MPMediaItemPropertyPlaybackDuration] = NSNumber(value: duration) }
        if let elapsed { info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = NSNumber(value: elapsed) }
        if let artwork { info[MPMediaItemPropertyArtwork] = artwork }

        infoCenter.nowPlayingInfo = info
        infoCenter.playbackState = (rate > 0) ? .playing : .paused
    }

    /// Lightweight elapsed-time refresh — called from the engine's periodic
    /// time-observer. Avoids rebuilding the whole info dict each tick.
    func updateElapsed(_ elapsed: TimeInterval, rate: Double) {
        guard var info = infoCenter.nowPlayingInfo else { return }
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = NSNumber(value: elapsed)
        info[MPNowPlayingInfoPropertyPlaybackRate] = NSNumber(value: rate)
        infoCenter.nowPlayingInfo = info
        infoCenter.playbackState = (rate > 0) ? .playing : .paused
    }

    /// Disable the entire remote command surface — used on teardown.
    func clear() {
        infoCenter.nowPlayingInfo = nil
        infoCenter.playbackState = .stopped
    }

    // MARK: - Command wiring

    private func wireCommandsIfNeeded() {
        guard !didWireCommands else {
            // Already wired; new callbacks are already in `self.callbacks`
            // and the closures below capture `self`, so they just work.
            return
        }
        didWireCommands = true

        commandCenter.playCommand.addTarget { [weak self] _ in
            self?.performRemoteCommand(.play) ?? .commandFailed
        }
        commandCenter.pauseCommand.addTarget { [weak self] _ in
            self?.performRemoteCommand(.pause) ?? .commandFailed
        }
        commandCenter.togglePlayPauseCommand.addTarget { [weak self] _ in
            self?.performRemoteCommand(.toggle) ?? .commandFailed
        }

        commandCenter.skipForwardCommand.preferredIntervals = [NSNumber(value: skipForwardSeconds)]
        commandCenter.skipForwardCommand.addTarget { [weak self] _ in
            self?.performRemoteCommand(.skipForward) ?? .commandFailed
        }

        commandCenter.skipBackwardCommand.preferredIntervals = [NSNumber(value: skipBackwardSeconds)]
        commandCenter.skipBackwardCommand.addTarget { [weak self] _ in
            self?.performRemoteCommand(.skipBackward) ?? .commandFailed
        }

        commandCenter.changePlaybackPositionCommand.isEnabled = true
        commandCenter.changePlaybackPositionCommand.addTarget { [weak self] event in
            guard let event = event as? MPChangePlaybackPositionCommandEvent else { return .commandFailed }
            return self?.performRemoteCommand(.seek(event.positionTime)) ?? .commandFailed
        }

        commandCenter.changePlaybackRateCommand.supportedPlaybackRates = [0.8, 1.0, 1.2, 1.5, 2.0]
        commandCenter.changePlaybackRateCommand.addTarget { [weak self] event in
            guard let event = event as? MPChangePlaybackRateCommandEvent else { return .commandFailed }
            return self?.performRemoteCommand(.changeRate(Double(event.playbackRate))) ?? .commandFailed
        }

        // Enable next/previous track so iOS delivers AirPods double/triple-tap
        // (and CarPlay / lock-screen next-prev presses) as `MPRemoteCommand`
        // events. `PlaybackState` decides what each maps to per the user's
        // headphone-gesture setting — see `HeadphoneGestureAction`.
        commandCenter.nextTrackCommand.isEnabled = true
        commandCenter.nextTrackCommand.addTarget { [weak self] _ in
            self?.performRemoteCommand(.nextTrack) ?? .commandFailed
        }
        commandCenter.previousTrackCommand.isEnabled = true
        commandCenter.previousTrackCommand.addTarget { [weak self] _ in
            self?.performRemoteCommand(.previousTrack) ?? .commandFailed
        }
    }
}
