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
    /// Tracks the accumulated seek target across rapid paused skip-forward taps.
    /// Reset on resume or when a position echoes back from Rust, so each tap
    /// builds on the previous rather than re-anchoring to a stale AVPlayer time.
    var pendingPausedSeekBase: Double?
    /// Episode whose Rust `AudioCommand::Load` callback has reached AudioEngine.
    /// Kernel snapshots are pulled asynchronously after dispatch, so the command
    /// handler uses this immediate local marker to avoid restaging an episode
    /// that Rust already loaded but has not yet projected.
    var rustLoadedEpisodeID: UUID?
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
        if !dispatchKernelLoad {
            // AudioCommand::Load callback path — Rust has resolved the streaming
            // URL. Load unconditionally: isSameEpisode is true here because the
            // user-dispatch path (below) already set episode = newEpisode before
            // calling kernelLoad, so a same-episode guard would silently skip
            // engine.load and AVPlayer would never receive the resolved URL.
            rustLoadedEpisodeID = newEpisode.id
            if isSameEpisode, engine.episode?.id == newEpisode.id {
                engine.refreshMetadata(for: newEpisode)
            } else {
                engine.load(newEpisode)
            }
            if newEpisode.playbackPosition > 0 {
                // TEMPORARY BYPASS: seeds the AVPlayer's initial position
                // before playback starts. The Rust kernel does not yet own a
                // "set initial playhead" primitive for this pre-play seam;
                // once it does, this direct engine call should be removed and
                // the position seeded via a kernel action instead.
                // BACKLOG: kernel-owned episode-load initial position (#599).
                engine.seek(to: newEpisode.playbackPosition)
            }
        } else if !isSameEpisode {
            // User-initiated load of a new episode — dispatch to Rust first so
            // policy and persistence stage in the kernel, then prime the iOS
            // executor locally. The Rust Load echo may still arrive later; the
            // callback path above refreshes same-episode metadata without
            // replacing the active AVPlayer item.
            rustLoadedEpisodeID = nil
            transport?.kernelLoad(episodeID: newEpisode.id)
            engine.load(newEpisode)
            if newEpisode.playbackPosition > 0 {
                engine.seek(to: newEpisode.playbackPosition)
            }
        } else {
            // Same episode, user-initiated — refresh metadata only.
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
        pendingPausedSeekBase = nil
        Haptics.medium()
        if shouldStartEpisodeThroughKernelPlay(episodeID: episode.id) {
            transport?.kernelPlay(episodeID: episode.id, startSeconds: nil, endSeconds: nil)
        } else {
            transport?.kernelResume()
        }
        if engine.episode?.id != episode.id {
            engine.load(episode)
            if episode.playbackPosition > 0 {
                engine.seek(to: episode.playbackPosition)
            }
        }
        engine.play()
    }

    func pause() {
        Haptics.soft()
        let pausedPosition = engine.currentTime
        transport?.kernelPause()
        // Flush the position to Rust's durable store so a force-quit
        // after pause doesn't lose the last-played position.
        if let episodeID = episode?.id {
            store?.kernelPersistPosition(episodeID: episodeID, positionSecs: pausedPosition)
        }
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
        if !isPlaying, let ep = episode {
            // Mirror of skipForward: use pendingPausedSeekBase so consecutive
            // back-taps while paused accumulate from the previous tap's target
            // rather than all anchoring to the same stale AVPlayer time.
            let base = pendingPausedSeekBase ?? engine.currentTime
            let target = max(0, base - delta)
            pendingPausedSeekBase = target
            transport?.kernelSeek(positionSecs: target)
            // apply_writeback won't run while paused — persist explicitly so a
            // kill-before-resume restores the correct position.
            store?.kernelPersistPosition(episodeID: ep.id, positionSecs: target)
            return
        }
        transport?.kernelSkipBackward(secs: delta)
    }

    func skipForward(_ seconds: TimeInterval? = nil) {
        let delta = seconds ?? engine.skipForwardSeconds
        if !isPlaying, let ep = episode {
            // When paused, Playing ticks don't fire so Rust's position_secs
            // stales out. Each rapid tap must accumulate from the *previous
            // tap's target*, not from AVPlayer's position (which only updates
            // when playing). pendingPausedSeekBase carries that running target
            // across taps; it is cleared on resume or when Rust echoes a seek.
            let base = pendingPausedSeekBase ?? engine.currentTime
            let target = min(base + delta, ep.duration ?? base + delta)
            pendingPausedSeekBase = target
            transport?.kernelSeek(positionSecs: target)
            // P2a: apply_writeback won't run while paused — persist explicitly
            // so a kill-before-resume restores the correct position.
            store?.kernelPersistPosition(episodeID: ep.id, positionSecs: target)
            return
        }
        transport?.kernelSkipForward(secs: delta)
    }

    func setRate(_ newRate: PlaybackRate) {
        let result = transport?.kernelSetSpeed(newRate.rawValue)
        applyAcceptedKernelSpeed(newRate.rawValue, result: result)
        Haptics.selection()
    }

    private func shouldStartEpisodeThroughKernelPlay(episodeID: UUID) -> Bool {
        guard store != nil else { return false }
        guard rustLoadedEpisodeID == episodeID else { return true }
        let staged = store?.kernel?.podcastSnapshot?.nowPlaying?.episodeId?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard let staged, !staged.isEmpty else { return true }
        return UUID(uuidString: staged) != episodeID
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
