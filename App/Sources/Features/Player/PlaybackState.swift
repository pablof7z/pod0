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
    /// `dispatchKernelLoad` controls whether a `kernelLoad` is forwarded to
    /// Rust. Pass `false` when called from the `AudioCommand::Load` handler —
    /// Rust already staged the episode; re-dispatching would echo a second
    /// Load+Play sequence (regression: playback resets to loading/paused).
    /// All user-initiated paths leave this at the default `true`.
    ///
    /// Idempotent: when `newEpisode.id` matches the current episode, skips
    /// the reload but still refreshes metadata and seeks to a saved resume
    /// point if the engine reached its natural end.
    func setEpisode(
        _ newEpisode: Episode,
        playAfterLoad: Bool = false,
        dispatchKernelLoad: Bool = true
    ) {
        let isSameEpisode = (episode?.id == newEpisode.id)
        episode = newEpisode
        if !isSameEpisode {
            // Stage the new episode in the Rust kernel first so that the
            // subsequent kernelResume() (from play()) operates on the correct
            // episode. Without this, the kernel may still have the previous
            // episode staged and resume the wrong item.
            // Skip when called from the AudioCommand::Load handler — Rust
            // already owns the episode; re-dispatching creates an echo loop.
            if dispatchKernelLoad {
                transport?.kernelLoad(episodeID: newEpisode.id)
            }
            engine.load(newEpisode)
            if newEpisode.playbackPosition > 0 {
                // TEMPORARY BYPASS: seeds the AVPlayer's initial position
                // before playback starts. The Rust kernel does not yet own a
                // "set initial playhead" primitive for this pre-play seam;
                // once it does, this direct engine call should be removed and
                // the position seeded via a kernel action instead.
                // BACKLOG: kernel-owned episode-load initial position (#599).
                engine.seek(to: newEpisode.playbackPosition)
            }
        } else {
            engine.refreshMetadata(for: newEpisode)
            if engine.didReachNaturalEnd {
                let resume = newEpisode.playbackPosition
                let target = resume > 0 && resume < max(0, duration - 5) ? resume : 0
                // TEMPORARY BYPASS: resets AVPlayer to the saved resume point
                // after a natural end-of-episode. Same limitation as above —
                // replace with a kernel action when the Rust player supports it.
                // BACKLOG: kernel-owned episode-load initial position (#599).
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
        // Write the seeked position durably so that if the app is killed while
        // paused after a scrub the next resume starts at the correct position
        // rather than snapping back to the last persisted value. When playing,
        // onPlayingTick / apply_writeback will overwrite this momentarily, so
        // the call is a benign no-op in that path.
        if let ep = episode {
            store?.kernelPersistPosition(episodeID: ep.id, positionSecs: time)
        }
        Haptics.selection()
    }

    func seekSnapping(to time: TimeInterval) { seek(to: time) }

    func skipBackward(_ seconds: TimeInterval? = nil) {
        let delta = seconds ?? engine.skipBackwardSeconds
        if !isPlaying {
            // When paused, Rust's PlayerActor.position_secs is not updated by
            // Playing reports. Sync it from AVPlayer's current time before
            // dispatching the skip so each consecutive tap accumulates from
            // the correct base instead of all computing from the same stale value.
            transport?.kernelSeek(positionSecs: engine.currentTime)
        }
        transport?.kernelSkipBackward(secs: delta)
        // P2a: No Playing ticks fire while paused, so apply_writeback never
        // saves the new position. Persist it explicitly so a kill-before-resume
        // restores the correct position rather than the pre-skip one.
        if !isPlaying, let ep = episode {
            store?.kernelPersistPosition(episodeID: ep.id, positionSecs: max(0, engine.currentTime - delta))
        }
    }

    func skipForward(_ seconds: TimeInterval? = nil) {
        let delta = seconds ?? engine.skipForwardSeconds
        if !isPlaying {
            // Same paused-accumulation fix as skipBackward: sync Rust's
            // position_secs from AVPlayer before each skip dispatch.
            transport?.kernelSeek(positionSecs: engine.currentTime)
        }
        transport?.kernelSkipForward(secs: delta)
        // P2a: Persist the new position while paused for the same reason as
        // skipBackward — apply_writeback won't fire until playback resumes.
        if !isPlaying, let ep = episode {
            let target = min(engine.currentTime + delta, ep.durationSecs ?? engine.currentTime + delta)
            store?.kernelPersistPosition(episodeID: ep.id, positionSecs: target)
        }
    }

    func setRate(_ newRate: PlaybackRate) {
        // Dispatch to Rust. Rust emits AudioCommand::SetSpeed, which the
        // commandHandler in PlaybackState+AudioCallbacks.swift routes to
        // engine.setRate — AudioEngine is the executor, not a secondary
        // writer here (#599).
        transport?.kernelSetSpeed(newRate.rawValue)
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
