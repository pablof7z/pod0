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

    /// Kernel playback transport dispatch seam. When non-nil, takes
    /// precedence over `store` for all transport dispatch calls. Set only
    /// in unit tests to inject a lightweight stub without subclassing the
    /// `final` `AppStateStore`. Nil in production.
    var kernelDispatch: (any KernelPlaybackDispatching)?

    /// Active kernel transport dispatcher: explicit injection or the store.
    /// Extensions that need to forward transport commands use this.
    var transport: (any KernelPlaybackDispatching)? { kernelDispatch ?? store }

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

    /// Kernel-projected authoritative queue. Sole writer: `applyKernelQueue(_:)`,
    /// called exclusively from `onQueueFromKernel`. User actions (remove, move,
    /// clear, prune) only dispatch to the kernel; the fast in-process round-trip
    /// updates this on the next snapshot tick. Read-only outside this class.
    var kernelQueue: [QueueItem] = []

    /// The rendered queue the UI reads. Pure read-only projection of the kernel
    /// queue — never locally mutated by Swift user actions.
    var queue: [QueueItem] { kernelQueue }

    /// Transient set of episode ids for which an enqueue was dispatched to the
    /// kernel and returned `.accepted`, but whose authoritative confirmation
    /// (via `onQueueFromKernel`) has not yet arrived. Drives the "Queued"
    /// button state between tap and the next kernel projection tick, giving
    /// instant feedback without making Swift a second writer to `queue`.
    /// Items are removed as each id appears in the kernel's queue projection.
    var pendingEnqueue: Set<UUID> = []
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
        transport?.kernelResume()
    }

    func pause() {
        Haptics.soft()
        transport?.kernelPause()
    }

    func seek(to time: TimeInterval) {
        transport?.kernelSeek(positionSecs: time)
        engine.seek(to: time)
        Haptics.selection()
    }

    func seekSnapping(to time: TimeInterval) { seek(to: time) }

    func skipBackward(_ seconds: TimeInterval? = nil) {
        let delta = seconds ?? engine.skipBackwardSeconds
        transport?.kernelSkipBackward(secs: delta)
    }

    func skipForward(_ seconds: TimeInterval? = nil) {
        let delta = seconds ?? engine.skipForwardSeconds
        transport?.kernelSkipForward(secs: delta)
    }

    func setRate(_ newRate: PlaybackRate) {
        // Dispatch to Rust first for policy and persistence. The resulting
        // AudioCommand::SetSpeed callback calls engine.setRate (idempotent).
        // Direct update immediately after for UI feedback so PlaybackState.rate
        // (which reads engine.rate) is current in the same render cycle.
        transport?.kernelSetSpeed(newRate.rawValue)
        engine.setRate(newRate.rawValue)
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
