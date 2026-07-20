import Foundation
import Observation
import Pod0Core
import SwiftUI

// MARK: - PlaybackState

/// Native presentation adapter around `AudioEngine` for the Player UI.
///
/// Rust owns durable playback state and policy. This type renders shared
/// projections, routes semantic controls to Rust, and exposes AVFoundation
/// presentation state without becoming a second source of truth.
@MainActor
@Observable
final class PlaybackState {

    // MARK: - Engine

    /// The single `AVPlayer`-backed engine.
    let engine: AudioEngine
    var productSignals: any ProductSignalSink

    // MARK: - Observable surface (matches the binding contract the UI expects)

    /// Currently-loaded episode, or `nil` when nothing has been queued.
    /// The `RootView` mini-bar reads this to decide whether to render itself.
    var episode: Episode?

    var sleepTimer: PlaybackSleepTimer = .off

    /// Up Next queue — ordered list of `QueueItem` values. Each item may carry
    /// optional `startSeconds`/`endSeconds` bounds for agent-curated segment
    /// playback. Full-episode items use `nil` for both fields.
    ///
    /// `NowPlayingTimelineProvider` reads only the current `episode` snapshot,
    /// not the queue, so widget metadata is unaffected by queue mutations.
    var queue: [QueueItem] = []

    /// When the currently-playing item is a bounded segment, this holds the
    /// episode-relative end boundary in seconds for presentation. Rust owns
    /// segment completion and queue advancement.
    var currentSegmentEndTime: Double? = nil

    /// Back-navigation stack populated by `navigationalSeek(to:)`.
    /// In-memory only (session-scoped, like browser history).
    var seekHistory: [SeekHistoryEntry] = []
    var canJumpBack: Bool { !seekHistory.isEmpty }

    /// Mirrors `AudioEngine.state` semantics through the lens the UI cares
    /// about: `playing` and `buffering` both render as "playing" so the
    /// play/pause glyph doesn't flicker through transient stalls.
    var isPlaying: Bool {
        switch engine.state {
        case .playing, .buffering: return true
        case .idle, .loading, .paused, .failed: return false
        }
    }

    /// Engine playhead, in seconds.
    var currentTime: TimeInterval { engine.currentTime }

    /// Engine duration. Falls back to the feed-supplied `Episode.duration` so
    /// the scrubber renders a sane width before `AVAsset` resolves the asset
    /// duration.
    var duration: TimeInterval {
        if engine.duration > 0 { return engine.duration }
        return episode?.duration ?? 0
    }

    /// Best-fit `PlaybackRate` for the engine's current rate. Reads always go
    /// through `engine.rate` so a remote `MPRemoteCommand` rate change still
    /// updates the UI.
    var rate: PlaybackRate {
        get { PlaybackRate.bestFit(for: engine.rate) }
        set { setRate(newValue) }
    }

    /// Called when `setEpisode` loads a *new* episode (`!isSameEpisode`)
    /// whose stable download evidence is `.notDownloaded`. Receivers
    /// should kick off the background download → transcription → chapters
    /// pipeline without blocking playback — the audio engine has already
    /// started streaming by the time this closure fires.
    ///
    /// Wired by `RootView` to `EpisodeDownloadService.ensureDownloadEnqueued`.
    /// The closure keeps `PlaybackState` decoupled from the download service
    /// while still funnelling every playback entry point
    /// (`play_episode`, Continue Listening, Home featured, deep links)
    /// through a single download trigger.
    ///
    /// Only fires on *new* episode load, never on same-episode reloads —
    /// Play/Resume taps and deep-link replays hit `setEpisode` on every
    /// gesture and would otherwise spam the download queue.
    var onEnsureDownloadEnqueued: (UUID) -> Void = { _ in }

    /// Resolves the parent show name for a given episode. Called by the
    /// snapshot writer so the widget can render the show subtitle without
    /// `PlaybackState` needing to know about `AppStateStore`. Returns `""`
    /// when the show name isn't known.
    var resolveShowName: (Episode) -> String = { _ in "" }

    /// Resolves the parent show's cover-art URL for a given episode. Used by
    /// the player UI as the fallback when `episode.imageURL` is `nil`.
    /// Mirrors the `resolveShowName` injection pattern so `PlaybackState`
    /// stays decoupled from `AppStateStore`. Returns `nil` when the show's
    /// artwork isn't known.
    var resolveShowImage: (Episode) -> URL? = { _ in nil }

    /// Headphone-gesture wiring. `resolveNavigableChapters` is set by
    /// `RootView` so chapter-aware actions see chapters as they hydrate.
    /// The two action fields mirror the matching `Settings` values via
    /// `applyPreferences`. `onClipRequested` fires when the configured
    /// action is `.clipNow`; `RootView` wires it to `AutoSnipController`.
    var resolveNavigableChapters: (Episode) -> [Episode.Chapter] = { _ in [] }
    var headphoneDoubleTapAction: HeadphoneGestureAction = .skipForward
    var headphoneTripleTapAction: HeadphoneGestureAction = .clipNow
    var onClipRequested: () -> Void = { }

    // MARK: - Internal

    /// Most recent App-Group snapshot write. Used to throttle position-only
    /// updates to once every 5 seconds — the widget's timeline refresh
    /// granularity makes finer writes wasted I/O.
    var lastSnapshotWrite: Date?
    @ObservationIgnored weak var sharedCore: SharedLibraryClient?
    @ObservationIgnored var chapterContext: ChapterPlaybackContext?
    @ObservationIgnored var pendingPlaySignal = false
    @ObservationIgnored var pendingResumeSignal: (episodeID: UUID, position: TimeInterval)?
    @ObservationIgnored var recordedMeaningfulEpisodeIDs: Set<UUID> = []

    var playbackFailure: UserFacingFailure? {
        guard case .failed(let error) = engine.state else { return nil }
        return UserFacingFailurePresenter.make(failure: error.failure, canRetry: true)
    }
    // MARK: - Init

    init(engine: AudioEngine = AudioEngine(), productSignals: any ProductSignalSink = DiscardingProductSignalSink.shared) {
        self.engine = engine
        self.productSignals = productSignals
        configureAudioEngineCallbacks()
    }
}
