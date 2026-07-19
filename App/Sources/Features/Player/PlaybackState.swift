import Foundation
import Observation
import Pod0Core
import SwiftUI

// MARK: - PlaybackState

/// Real, observable wrapper around `AudioEngine` that the Player UI binds to.
///
/// Owns one `AudioEngine`, renders shared playback projections, and routes
/// semantic controls to Rust. The explicitly disabled/pre-cutover path retains
/// its characterized Swift persistence callbacks until cleanup issue #83.
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
    /// segment completion in shared mode; the legacy loop reads this only in
    /// explicitly disabled/pre-cutover mode.
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

    // MARK: - Persistence hooks (wired by RootView at .onAppear time)

    /// Called once per second while playback advances.
    var onPersistPosition: (UUID, TimeInterval) -> Void = { _, _ in }

    /// Called once per episode when the playhead reaches the end.
    var onEpisodeFinished: (UUID) -> Void = { _ in }

    /// Called when the playhead reaches `currentSegmentEndTime` for a bounded
    /// segment item. `RootView` wires this to `playNext` (with `pause()` as
    /// fallback when the queue is empty) so the transition logic lives in one
    /// place. Not fired for full-episode items — those go through `onEpisodeFinished`.
    var onSegmentFinished: () -> Void = { }

    /// Called when the player wants any queued position writes drained to
    /// disk synchronously: on pause, on natural end-of-episode (so the
    /// final position survives even when auto-mark-played is off), and on
    /// episode change (so the previous episode's position is durable
    /// before the next episode steals the persistence loop).
    ///
    /// Wired by `RootView` to `AppStateStore.flushPendingPositions`. The
    /// store also flushes on `UIApplication.didEnterBackgroundNotification`
    /// independently, so this closure is for the in-app transitions the
    /// store can't observe directly.
    var onFlushPositions: () -> Void = { }

    /// Called when `setEpisode` loads a *new* episode (`!isSameEpisode`)
    /// whose stable download evidence is `.notDownloaded`. Receivers
    /// should kick off the background download → transcription → chapters
    /// pipeline without blocking playback — the audio engine has already
    /// started streaming by the time this closure fires.
    ///
    /// Wired by `RootView` to `EpisodeDownloadService.ensureDownloadEnqueued`.
    /// The closure injection mirrors `onPersistPosition` / `onFlushPositions`
    /// so `PlaybackState` stays decoupled from the download service for
    /// tests, while still funnelling every playback entry point
    /// (`play_episode`, Continue Listening, Home featured, deep links)
    /// through a single download trigger.
    ///
    /// Only fires on *new* episode load, never on same-episode reloads —
    /// Play/Resume taps and deep-link replays hit `setEpisode` on every
    /// gesture and would otherwise spam the download queue.
    var onEnsureDownloadEnqueued: (UUID) -> Void = { _ in }

    /// Mirrors `Settings.autoMarkPlayedAtEnd`. When `false`, end-of-item
    /// detection still stops the persistence loop from over-writing the
    /// final position but skips the `onEpisodeFinished` callback.
    var autoMarkPlayedOnFinish: Bool = true

    /// Mirrors `Settings.autoSkipAds`. When `true`, `tickPersistence` seeks
    /// past any `Episode.AdSegment` the playhead enters, throttled to one
    /// skip per segment per playback session via `skippedAdSegmentIDs`.
    /// Off by default so the toggle stays opt-in until detection quality
    /// is proven.
    var autoSkipAdsEnabled: Bool = false

    /// Ad segments for the currently-loaded episode. Refreshed by
    /// `RootView` whenever the episode changes (and after detection runs)
    /// so the auto-skip loop doesn't have to reach into `AppStateStore`
    /// from a tight 1-second tick. Empty when detection hasn't run or
    /// found nothing.
    var adSegments: [Episode.AdSegment] = []

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

    /// Drives the 1-second persistence + end-detection loop.
    var persistenceTask: Task<Void, Never>?
    /// Prevents `onEpisodeFinished` from firing twice for the same playthrough.
    var didFireFinishedFor: UUID?
    /// Most recent App-Group snapshot write. Used to throttle position-only
    /// updates to once every 5 seconds — the widget's timeline refresh
    /// granularity makes finer writes wasted I/O.
    var lastSnapshotWrite: Date?
    /// Ad segments already auto-skipped in this playback session, keyed by
    /// `AdSegment.id`. Cleared on episode change so a user replaying the
    /// same episode sees ads skipped again. Not persisted — purely
    /// throttling state for the 1-second tick loop.
    var skippedAdSegmentIDs: Set<UUID> = []
    var sessionPolicy = PlaybackSessionPolicy()
    var playbackRequested = false
    var lastHostObservation: PlaybackLifecycleObservation?
    var onHostObservation: (PlaybackLifecycleObservation) -> Void = { _ in }
    @ObservationIgnored weak var sharedCore: SharedLibraryClient?
    // MARK: - Init

    init(engine: AudioEngine = AudioEngine(), productSignals: any ProductSignalSink = DiscardingProductSignalSink.shared) {
        self.engine = engine
        self.productSignals = productSignals
        configureAudioEngineCallbacks()
    }
}
