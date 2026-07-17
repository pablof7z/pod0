import Foundation
import Observation
import SwiftUI

// MARK: - PlaybackState

/// Real, observable wrapper around `AudioEngine` that the Player UI binds to.
///
/// Owns a single `AudioEngine` instance and republishes its state through
/// `@Observable` properties so SwiftUI re-renders on changes. Also: throttles
/// a 1-second persistence mirror, detects end-of-episode, and adapts the
/// engine's `SleepTimer.Mode` to the UI's preset enum. Persistence wires via
/// closures (`onPersistPosition`, `onEpisodeFinished`) so the type stays
/// testable without holding an `AppStateStore` reference directly.
@MainActor
@Observable
final class PlaybackState {

    // MARK: - Engine

    /// The single `AVPlayer`-backed engine. Held here so SwiftUI views can also
    /// reach into `engine.sleepTimer.phase` for countdown rendering.
    let engine: AudioEngine

    // MARK: - Observable surface (matches the binding contract the UI expects)

    /// Currently-loaded episode, or `nil` when nothing has been queued.
    /// The `RootView` mini-bar reads this to decide whether to render itself.
    var episode: Episode?

    var sleepTimer: PlaybackSleepTimer = .off

    /// Live label for the sleep-timer action chip. Renders the live countdown
    /// when armed in duration mode so the chip reads "29:42" and ticks down
    /// — was previously stuck on the static preset string ("30 min") for the
    /// entire armed window. Read from a SwiftUI view body so @Observable
    /// dependency tracking picks up the engine's per-tick phase changes.
    var sleepTimerChipLabel: String {
        switch engine.sleepTimer.phase {
        case .idle:
            return "Sleep"
        case .armed(let remaining), .fading(let remaining):
            return Self.formatRemaining(remaining)
        case .armedEndOfEpisode:
            return "End"
        case .fired:
            return "Sleep"
        }
    }

    /// `mm:ss` for under an hour, `h:mm:ss` otherwise. Negative / zero values
    /// floor to "0:00" so a brief race during the fade-to-fire transition
    /// doesn't print "-1".
    private static func formatRemaining(_ seconds: TimeInterval) -> String {
        let total = max(0, Int(seconds.rounded(.up)))
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, s)
            : String(format: "%d:%02d", m, s)
    }

    /// Up Next queue — ordered list of `QueueItem` values. Each item may carry
    /// optional `startSeconds`/`endSeconds` bounds for agent-curated segment
    /// playback. Full-episode items use `nil` for both fields.
    ///
    /// `NowPlayingTimelineProvider` reads only the current `episode` snapshot,
    /// not the queue, so widget metadata is unaffected by queue mutations.
    var queue: [QueueItem] = []

    /// When the currently-playing item is a bounded segment, this holds the
    /// episode-relative end boundary in seconds. `tickPersistence` watches this
    /// and calls `onSegmentFinished` when the playhead crosses it, then clears
    /// it. `nil` for full-episode items (natural-end detection applies instead).
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
        set { engine.setRate(newValue.rawValue) }
    }

    // MARK: - Persistence hooks (wired by RootView at .onAppear time)

    /// Called once per second while playback advances. Receivers should
    /// persist the playhead so the user resumes where they left off across
    /// app launches.
    var onPersistPosition: (UUID, TimeInterval) -> Void = { _, _ in }

    /// Called once per episode when the playhead reaches the end. Receivers
    /// should mark the episode as fully played. Gated by `autoMarkPlayedOnFinish`
    /// (mirrors `Settings.autoMarkPlayedAtEnd`) so the user can opt out of auto-mark.
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
    /// whose `downloadState` is `.notDownloaded` or `.failed`. Receivers
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
    private var persistenceTask: Task<Void, Never>?
    /// Prevents `onEpisodeFinished` from firing twice for the same playthrough.
    private var didFireFinishedFor: UUID?
    /// Most recent App-Group snapshot write. Used to throttle position-only
    /// updates to once every 5 seconds — the widget's timeline refresh
    /// granularity makes finer writes wasted I/O.
    var lastSnapshotWrite: Date?
    /// Ad segments already auto-skipped in this playback session, keyed by
    /// `AdSegment.id`. Cleared on episode change so a user replaying the
    /// same episode sees ads skipped again. Not persisted — purely
    /// throttling state for the 1-second tick loop.
    var skippedAdSegmentIDs: Set<UUID> = []

    // MARK: - Init

    init(engine: AudioEngine = AudioEngine()) {
        self.engine = engine
        configureAudioEngineCallbacks()
    }

    // MARK: - Episode lifecycle

    /// Replace the current item with `newEpisode`. Resumes from the persisted
    /// `playbackPosition` when present. Caller must follow with `play()` to
    /// actually start audio — matches the engine's deliberate two-step flow.
    ///
    /// **Idempotent.** When `newEpisode.id` matches the currently-loaded
    /// episode, skip the `engine.load` reload — it would replace the
    /// `AVPlayerItem` and interrupt in-flight playback for a caller that
    /// just wanted "make sure this is loaded" semantics (the EpisodeDetail
    /// hero "Play/Resume" button, chapter-row taps, deep-links). The
    /// metadata refresh + snapshot write still run so any post-hydrate
    /// changes (chapters, title) flush to the widget.
    func setEpisode(_ newEpisode: Episode) {
        let isSameEpisode = (episode?.id == newEpisode.id)
        if !isSameEpisode {
            // Drain any cached position for the previous episode before
            // we steal the persistence loop — otherwise the outgoing
            // playhead would only land on disk at the next 30s eager-cap
            // tick, by which time the user may have force-quit.
            onFlushPositions()
            didFireFinishedFor = nil
            lastSnapshotWrite = nil
            // Skipped-ad set is per-episode-session. Replaying the same
            // episode should re-skip the same ads; a brand-new episode
            // starts with an empty set.
            skippedAdSegmentIDs = []
        } else {
            // Same-id reload (Play/Resume tap, deep-link, chapter-row).
            // Clear the finished-flag so a user replaying an already-
            // finished episode resumes producing position writes — without
            // this, `tickPersistence` returns immediately on the
            // `didFireFinishedFor` guard and the new playthrough is
            // entirely lost on force-quit.
            didFireFinishedFor = nil
        }
        episode = newEpisode
        // Refresh the local ad-segments cache from the newly-loaded episode
        // so the 1-second auto-skip loop has the right list. On same-episode
        // reloads we still refresh — detection may have completed since the
        // previous `setEpisode` call and added segments to the model.
        adSegments = newEpisode.adSegments ?? []
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
        // Episode change is the one event that always justifies a snapshot
        // write — title and artwork just changed, so the widget would
        // otherwise show stale metadata until the next 5-second tick. For
        // same-episode calls we still want the refresh in case chapter
        // hydration or feed-refresh updated the metadata while playback
        // was rolling.
        writeNowPlayingSnapshot(force: true)
        if !isSameEpisode {
            startPersistenceLoop()
            // Kick off the background download → transcription → chapters
            // pipeline for any episode the user streams that isn't yet on
            // disk. Gated on `!isSameEpisode` because same-episode reloads
            // (Play/Resume taps, deep-link replays, chapter-row taps) fire
            // on every gesture and would otherwise spam the queue. The
            // receiver (RootView → EpisodeDownloadService) early-returns
            // for `.downloading` / `.queued` / `.downloaded` so the
            // common case after the first play is a cheap no-op.
            switch newEpisode.downloadState {
            case .notDownloaded, .failed:
                onEnsureDownloadEnqueued(newEpisode.id)
            case .downloading, .queued, .downloaded:
                break
            }
        }
    }

    // MARK: - Imperative methods (binding contract for the player UI)

    func togglePlayPause() {
        if isPlaying {
            pause()
        } else {
            play()
        }
    }

    func play() {
        guard episode != nil else { return }
        Haptics.medium()
        engine.play()
        startPersistenceLoop()
        // Force-write the snapshot so the widget's play/pause glyph
        // flips immediately — the throttled persistence-loop write would
        // otherwise lag up to 5s.
        writeNowPlayingSnapshot(force: true)
    }

    func pause() {
        Haptics.soft()
        let pausedEpisodeID = episode?.id
        if engine.didReachNaturalEnd {
            tickPersistence()
        }
        guard episode?.id == pausedEpisodeID else { return }
        engine.pause()
        // Stop the 1-second persistence + snapshot loop while paused —
        // otherwise it keeps re-writing the same `currentTime` and
        // bouncing widget timelines for nothing, and races with the
        // pause flush below in pathological force-quit windows.
        // `play()` restarts the loop.
        persistenceTask?.cancel()
        persistenceTask = nil
        // Pause is a "the user is done for now" signal — drain the
        // position cache so the playhead survives a force-quit-after-
        // pause cycle. Cheap when the cache is empty.
        onFlushPositions()
        // Same reasoning as `play()` — keep the widget's glyph in sync
        // with the engine state without waiting on the next tick.
        writeNowPlayingSnapshot(force: true)
    }

    func seek(to time: TimeInterval) {
        engine.seek(to: time)
        Haptics.selection()
        persistAndFlushAfterUserSeek()
    }

    /// `seekSnapping` was a transcript-snap behaviour in the mock. With the
    /// transcript stubbed (lane-3 pending) it now just delegates to `seek`.
    func seekSnapping(to time: TimeInterval) {
        seek(to: time)
    }

    /// Skip backwards. Pass `nil` (the default) to honour the user's configured
    /// `skipBackwardSeconds` from `Settings`. Pass an explicit value when a UI
    /// gesture wants a specific delta (e.g. transcript chapter rewind).
    func skipBackward(_ seconds: TimeInterval? = nil) {
        engine.skip(back: seconds)
        persistAndFlushAfterUserSeek()
    }

    /// Skip forward. Pass `nil` (the default) to honour the user's configured
    /// `skipForwardSeconds` from `Settings`.
    func skipForward(_ seconds: TimeInterval? = nil) {
        engine.skip(forward: seconds)
        persistAndFlushAfterUserSeek()
    }

    /// Persists the post-seek position immediately and drains the cache.
    ///
    /// Without this, a user who scrubs / skips and then force-quits within
    /// the 30s position-debounce window resumes from the **pre-seek**
    /// position — the engine moved the playhead but the cache hadn't been
    /// touched yet (`tickPersistence` runs on a 1s timer). A user-initiated
    /// position change is the most explicit "remember where I am" signal we
    /// get; treat it like pause and flush eagerly.
    func persistAndFlushAfterUserSeek() {
        guard let episode else { return }
        let time = engine.currentTime
        if time > 0 {
            onPersistPosition(episode.id, time)
        }
        onFlushPositions()
    }

    func setRate(_ newRate: PlaybackRate) {
        engine.setRate(newRate.rawValue)
        Haptics.selection()
    }

    /// Effective skip intervals (read from the engine so the lock-screen and
    /// in-app transport always agree). Surfaced for the player UI to render
    /// the right `gobackward.NN` / `goforward.NN` glyph and the matching
    /// accessibility label.
    var skipForwardSeconds: Int { Int(engine.skipForwardSeconds) }
    var skipBackwardSeconds: Int { Int(engine.skipBackwardSeconds) }

    /// Push live `Settings` values into the engine. Called by `RootView` on
    /// `.onAppear` and again whenever `state.settings` changes so a Settings
    /// edit takes effect immediately on the lock-screen and the in-app transport.
    func applyPreferences(from settings: Settings) {
        engine.skipForwardSeconds = Double(max(1, settings.skipForwardSeconds))
        engine.skipBackwardSeconds = Double(max(1, settings.skipBackwardSeconds))
        // Default rate only takes effect for items that haven't been started.
        // Once the user nudges the speed sheet we don't want to clobber their
        // choice on every settings change, so we only reset when the engine is
        // still at its baseline rate.
        if engine.episode == nil {
            engine.setRate(settings.defaultPlaybackRate)
        }
        // Mirror the user's auto-skip-ads preference. The 1-second
        // persistence loop reads `autoSkipAdsEnabled` directly so a Settings
        // edit takes effect on the next tick — no need to re-open the player.
        autoSkipAdsEnabled = settings.autoSkipAds
        headphoneDoubleTapAction = settings.headphoneDoubleTapAction
        headphoneTripleTapAction = settings.headphoneTripleTapAction
    }

    func setSleepTimer(_ timer: PlaybackSleepTimer) {
        sleepTimer = timer
        engine.setSleepTimer(timer.engineMode)
        Haptics.selection()
    }

    // MARK: - Persistence loop

    /// Polls `engine.currentTime` once per second and forwards to the persistence
    /// closure. A separate path detects end-of-episode so the store can flip
    /// `played = true` without subscribing to the engine's internal observer.
    private func startPersistenceLoop() {
        persistenceTask?.cancel()
        persistenceTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self else { return }
                self.tickPersistence()
            }
        }
    }

    private func tickPersistence() {
        guard let episode else { return }
        // Once the episode is marked finished, stop touching its position —
        // otherwise we'd persist `currentTime == duration` right back over the
        // store-side reset that `markEpisodePlayed` performed.
        guard didFireFinishedFor != episode.id else { return }

        let time = engine.currentTime
        if time > 0 {
            onPersistPosition(episode.id, time)
        }

        // Segment end detection — checked before natural-end so a bounded
        // agent segment transitions cleanly without marking the episode played.
        // `currentSegmentEndTime` is cleared first to prevent re-firing on the
        // next tick if `onSegmentFinished` is slow to advance the queue.
        if let segEnd = currentSegmentEndTime, time >= segEnd {
            currentSegmentEndTime = nil
            onSegmentFinished()
            return
        }

        applyAutoSkipAdsIfNeeded(at: time)
        writeNowPlayingSnapshot(force: false)

        if engine.didReachNaturalEnd {
            didFireFinishedFor = episode.id
            if autoMarkPlayedOnFinish {
                onEpisodeFinished(episode.id)
            } else {
                onFlushPositions()
            }
        }
    }
}
