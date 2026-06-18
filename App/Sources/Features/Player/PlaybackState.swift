import Foundation
import Observation
import SwiftUI

// MARK: - PlaybackState
//
// Observable wrapper around `AudioEngine` that the Player UI binds to.
// Owns the `AudioEngine` instance, republishes its state through
// `@Observable` properties, and dispatches player commands to both the
// engine and the Rust kernel.
//
// Persistence, end-detection, ad-skip, and auto-advance are owned by
// Rust (`audio_report.rs`). Reports flow: AudioEngine → AudioCapability
// → Rust via the kernel bridge wired in `+AudioCallbacks.swift`.
@MainActor
@Observable
final class PlaybackState {

    // MARK: - Engine + store

    let engine: AudioEngine

    /// Injected by RootView at `.onAppear`. Used for kernel dispatch
    /// (queue persistence and playback state) without a retained cycle.
    weak var store: AppStateStore?

    // MARK: - Observable surface

    var episode: Episode?
    var sleepTimer: PlaybackSleepTimer = .off

    var sleepTimerChipLabel: String {
        if store?.kernel?.podcastSnapshot?.nowPlaying?.sleepTimerEndOfEpisode == true {
            return "End"
        }
        if let remaining = store?.kernel?.podcastSnapshot?.nowPlaying?.sleepTimerRemainingSecs {
            return Self.formatRemaining(TimeInterval(remaining))
        }
        return "Sleep"
    }

    private static func formatRemaining(_ seconds: TimeInterval) -> String {
        let total = max(0, Int(seconds.rounded(.up)))
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, s)
            : String(format: "%d:%02d", m, s)
    }

    var queue: [QueueItem] = []
    var seekHistory: [SeekHistoryEntry] = []
    var canJumpBack: Bool { !seekHistory.isEmpty }

    var isPlaying: Bool {
        switch engine.state {
        case .playing, .buffering: return true
        case .idle, .loading, .paused, .failed: return false
        }
    }

    var currentTime: TimeInterval { engine.currentTime }

    var duration: TimeInterval {
        if engine.duration > 0 { return engine.duration }
        return episode?.duration ?? 0
    }

    var rate: PlaybackRate {
        get { PlaybackRate.bestFit(for: engine.rate) }
        set { setRate(newValue) }
    }

    // MARK: - Headphone gesture config

    var headphoneDoubleTapAction: HeadphoneGestureAction = .skipForward
    var headphoneTripleTapAction: HeadphoneGestureAction = .clipNow

    // MARK: - Init

    init(engine: AudioEngine = AudioEngine()) {
        self.engine = engine
        configureAudioEngineCallbacks()
    }

    // MARK: - Episode lifecycle

    /// Load `newEpisode` into the engine. When `playAfterLoad` is true
    /// (the default for user-initiated play), also calls `play()`.
    ///
    /// Idempotent: when `newEpisode.id` matches the current episode, skips
    /// the reload but still refreshes metadata and seeks to a saved resume
    /// point if the engine reached its natural end.
    func setEpisode(
        _ newEpisode: Episode,
        playAfterLoad: Bool = false
    ) {
        let isSameEpisode = (episode?.id == newEpisode.id)
        episode = newEpisode
        if !isSameEpisode {
            engine.load(newEpisode)
            if newEpisode.playbackPosition > 0 {
                engine.seek(to: newEpisode.playbackPosition)
            }
        } else {
            engine.refreshMetadata(for: newEpisode)
            if engine.didReachNaturalEnd {
                let resume = newEpisode.playbackPosition
                let target = resume > 0 && resume < max(0, duration - 5) ? resume : 0
                engine.seek(to: target)
            }
        }
        if playAfterLoad { play() }
    }

    // MARK: - Playback controls

    func togglePlayPause() {
        if isPlaying { pause() } else { play() }
    }

    func play() {
        guard let episode else { return }
        Haptics.medium()
        engine.play()
        store?.kernelLoad(episodeID: episode.id)
    }

    func pause() {
        Haptics.soft()
        engine.pause()
    }

    func seek(to time: TimeInterval) {
        engine.seek(to: time)
        Haptics.selection()
        // When paused, Playing reports aren't flowing so Rust's saved position
        // would be stale. PersistPosition writes directly to the store (no
        // audio command returned) so the next play() → kernelLoad returns the
        // correct resume point and doesn't snap the engine back.
        if !isPlaying, let episodeID = episode?.id {
            store?.kernelPersistPosition(episodeID: episodeID, positionSecs: time)
        }
    }

    func seekSnapping(to time: TimeInterval) { seek(to: time) }

    func skipBackward(_ seconds: TimeInterval? = nil) {
        let delta = seconds ?? engine.skipBackwardSeconds
        let target = max(engine.currentTime - delta, 0)
        engine.skip(back: seconds)
        if !isPlaying, let episodeID = episode?.id {
            store?.kernelPersistPosition(episodeID: episodeID, positionSecs: target)
        }
    }

    func skipForward(_ seconds: TimeInterval? = nil) {
        let delta = seconds ?? engine.skipForwardSeconds
        let target = min(engine.currentTime + delta, duration)
        engine.skip(forward: seconds)
        if !isPlaying, let episodeID = episode?.id {
            store?.kernelPersistPosition(episodeID: episodeID, positionSecs: target)
        }
    }

    func setRate(_ newRate: PlaybackRate) {
        // Update the engine immediately so the UI reflects the new rate
        // without waiting for the Rust kernel's async capability round-trip.
        // kernelSetSpeed dispatches to Rust for persistence and AVPlayer sync;
        // the resulting AudioCommand::SetSpeed callback calls engine.setRate
        // again (idempotent). The direct update here ensures PlaybackState.rate
        // (which reads engine.rate) is current in the same render cycle.
        engine.setRate(newRate.rawValue)
        store?.kernelSetSpeed(newRate.rawValue)
        Haptics.selection()
    }

    var skipForwardSeconds: Int { Int(engine.skipForwardSeconds) }
    var skipBackwardSeconds: Int { Int(engine.skipBackwardSeconds) }

    func applyPreferences(from settings: Settings) {
        engine.skipForwardSeconds = Double(max(1, settings.skipForwardSeconds))
        engine.skipBackwardSeconds = Double(max(1, settings.skipBackwardSeconds))
        if engine.episode == nil {
            engine.setRate(settings.defaultPlaybackRate)
        }
        headphoneDoubleTapAction = settings.headphoneDoubleTapAction
        headphoneTripleTapAction = settings.headphoneTripleTapAction
    }

    func setSleepTimer(_ timer: PlaybackSleepTimer) {
        sleepTimer = timer
        store?.kernelSetSleepTimer(timer)
        Haptics.selection()
    }
}
