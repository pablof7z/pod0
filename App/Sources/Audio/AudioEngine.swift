import AVFoundation
import Combine
import Foundation
import Kingfisher
import MediaPlayer
import os.log
import UIKit

// MARK: - EngineError

/// `Error`-conforming + `Equatable` so the engine's `State` enum can stay
/// `Equatable` for SwiftUI diffing.
struct EngineError: Error, Equatable, Sendable, CustomStringConvertible {
    let message: String
    init(_ message: String) { self.message = message }
    var description: String { message }
}

// MARK: - AudioEngine

/// Wraps a single `AVPlayer`, exposes an `@Observable` playback state, and
/// brokers commands from the player UI, the agent, and Now Playing controls.
///
/// Owns:
/// - The active `AVPlayer` and its `AVPlayerItem`
/// - `AudioSessionCoordinator` activation for `.podcastPlayback`
/// - A `NowPlayingCenter` instance (lock-screen + Control Center bridge)
/// - A `SleepTimer` (duration / end-of-episode / fade-out)
///
/// Lifecycle: `idle → loading(Episode) → playing | paused → buffering → playing`,
/// with `failed(EngineError)` reachable from any state. Observer wiring lives
/// in `AudioEngine+Observers.swift` to stay under the 300-line soft limit.
@MainActor
@Observable
final class AudioEngine {

    // MARK: - State

    enum State: Equatable, Sendable {
        case idle
        case loading(Episode)
        case playing
        case paused
        case buffering
        case failed(EngineError)
    }

    // MARK: - Observable surface

    private(set) var state: State = .idle
    private(set) var currentTime: TimeInterval = 0
    private(set) var duration: TimeInterval = 0
    private(set) var rate: Double = 1.0
    private(set) var episode: Episode?

    /// `true` once the natural end-of-item observer has fired for the current
    /// episode. Distinguishes "user paused at 99.9 % of duration" from "episode
    /// genuinely finished" — the two are otherwise indistinguishable from a
    /// `currentTime`/`state` snapshot, and a 100 ms tolerance for jitter would
    /// otherwise auto-mark a manually-paused episode as played. Reset by
    /// `load(_:)` and by any user-initiated seek that lands more than 5 s
    /// before the end.
    ///
    /// Setter is module-internal (not `private(set)`) so the
    /// `AudioEngine+Observers` extension — which lives in a sibling file —
    /// can flip it from `handleEndOfItem`.
    var didReachNaturalEnd: Bool = false

    /// Sleep-timer surface so the player UI can render the countdown.
    let sleepTimer = SleepTimer()

    /// Called when `SleepTimer` fires. `PlaybackState` overrides this so timer
    /// pauses travel through the same persistence and snapshot side effects as
    /// an in-app pause.
    var onSleepTimerFire: () -> Void = {}

    /// NowPlaying surface — exposed so the player can push artwork mid-playback
    /// once Lane 4 has it loaded (artwork isn't on `Episode` yet — Lane 2 owns).
    let nowPlaying = NowPlayingCenter()

    // MARK: - Tunables

    /// Asymmetric defaults — see baseline-podcast-features.md (forward 30s, back 15s).
    var skipForwardSeconds: Double = 30 {
        didSet { nowPlaying.setSkipIntervals(forward: skipForwardSeconds, backward: skipBackwardSeconds) }
    }
    var skipBackwardSeconds: Double = 15 {
        didSet { nowPlaying.setSkipIntervals(forward: skipForwardSeconds, backward: skipBackwardSeconds) }
    }

    // MARK: - Now Playing metadata resolvers
    //
    // Closures injected by `RootView` so the lock-screen / Control Center
    // metadata can show the show name and active chapter title without
    // coupling the engine to `AppStateStore`. Each defaults to a no-op so
    // the engine works in isolation (unit tests, previews).

    /// Returns the show (subscription) title for an episode. Surfaces as the
    /// lock-screen `MPMediaItemPropertyArtist` line.
    var resolveShowName: (Episode) -> String? = { _ in nil }

    /// Returns the active chapter title at `playhead`, when the live episode
    /// has navigable chapters. Surfaces as the lock-screen
    /// `MPMediaItemPropertyAlbumTitle` line. Pass-through closure so the
    /// engine doesn't have to know how chapters are stored.
    var resolveActiveChapterTitle: (Episode, TimeInterval) -> String? = { _, _ in nil }

    /// Returns the artwork URL to render on the lock screen for the current
    /// playhead — chapter image takes precedence over episode/show artwork
    /// so the system surface mirrors the in-app hero. Returns `nil` when no
    /// artwork is available.
    var resolveArtworkURL: (Episode, TimeInterval) -> URL? = { _, _ in nil }

    /// Most-recently-published chapter title — checked on each time-observer
    /// tick so a chapter boundary crossing triggers a full nowPlaying republish
    /// (the lightweight `updateElapsed` path only refreshes elapsed/rate).
    var lastPublishedChapterTitle: String?

    /// Most-recently-resolved artwork URL — used to avoid redundant
    /// Kingfisher fetches when the URL hasn't changed (chapter title may
    /// flip without the artwork URL flipping).
    var lastPublishedArtworkURL: URL?

    /// Cached UIImage backing the last-published `MPMediaItemArtwork`. The
    /// artwork's request handler returns it (resized) on demand by the
    /// media center.
    var lastPublishedArtworkImage: UIImage?

    // MARK: - Internal (shared with AudioEngine+Observers.swift)

    let logger = Logger.app("AudioEngine")
    let player = AVPlayer()
    var timeObserverToken: Any?
    var statusObservation: NSKeyValueObservation?
    var timeControlObservation: NSKeyValueObservation?
    var bufferEmptyObservation: NSKeyValueObservation?
    var bufferLikelyToKeepUpObservation: NSKeyValueObservation?
    var endObserver: NSObjectProtocol?
    var fadeBaseVolume: Float = 1.0

    /// Per-effect multiplier that composes into `player.volume` via
    /// `applyEffectiveVolume`. Sleep timer drives `sleepFadeMultiplier`.
    private var sleepFadeMultiplier: Float = 1.0

    // MARK: - Init / deinit

    init() {
        configureNowPlayingCallbacks()
        onSleepTimerFire = { [weak self] in self?.pause() }
        configureSleepTimerHooks()
        nowPlaying.setSkipIntervals(forward: skipForwardSeconds, backward: skipBackwardSeconds)
    }

    // Note: no `deinit` cleanup. Under Swift 6 strict concurrency, `deinit` is
    // nonisolated and cannot touch `@MainActor` properties. `AVPlayer` releases
    // its time observer on deallocation; the `NotificationCenter` token also
    // dies with the engine. Explicit teardown happens in `teardownItemObservers()`
    // when a new episode loads.

    // MARK: - Public API

    /// Replace the current item with `episode`. Begins buffering immediately;
    /// caller must follow with `play()` to start playback.
    ///
    /// Prefers the locally-downloaded enclosure when available. We recompute
    /// the local path from `EpisodeDownloadStore` (rather than trusting the
    /// `localFileURL` baked into `DownloadState.downloaded`) because iOS may
    /// rotate the app container path across launches, leaving the persisted
    /// absolute URL stale. Falls back to streaming when no local file exists.
    func load(_ episode: Episode) {
        let url: URL = {
            if EpisodeDownloadStore.shared.exists(for: episode) {
                return EpisodeDownloadStore.shared.localFileURL(for: episode)
            }
            // Agent-generated episodes live under agent-episodes/, not downloads/.
            // Recompute the path fresh so stale container-relative absolute URLs
            // (persisted from a previous launch) are never handed to AVPlayer.
            if case .downloaded = episode.downloadState,
               let freshURL = try? AgentGeneratedPodcastService.audioFileURL(episodeID: episode.id),
               FileManager.default.fileExists(atPath: freshURL.path) {
                return freshURL
            }
            return episode.enclosureURL
        }()
        teardownItemObservers()
        self.episode = episode
        state = .loading(episode)
        currentTime = 0
        duration = episode.duration ?? 0
        didReachNaturalEnd = false

        let asset = AVURLAsset(url: url)
        let item = AVPlayerItem(asset: asset)
        player.replaceCurrentItem(with: item)
        installItemObservers(for: item)
        installTimeObserver()

        publishNowPlaying()
    }

    /// Refresh metadata for the already-loaded episode without replacing the
    /// `AVPlayerItem`. Used when the store rehydrates chapters/artwork/title
    /// for the same episode while audio keeps rolling.
    func refreshMetadata(for refreshed: Episode) {
        let previousFeedDuration = episode?.duration
        episode = refreshed
        if let refreshedDuration = refreshed.duration {
            if duration <= 0 {
                duration = refreshedDuration
            } else if let previousFeedDuration, duration == previousFeedDuration {
                duration = refreshedDuration
            }
        }
        publishNowPlaying()
    }

    /// Start playback. Activates the audio session lazily on first play so the
    /// app doesn't preempt other audio at launch.
    func play() {
        guard episode != nil else { return }
        do {
            try AudioSessionCoordinator.shared.activate(.podcastPlayback)
        } catch {
            logger.error("Failed to activate audio session: \(error, privacy: .public)")
            state = .failed(EngineError("Could not activate audio session: \(error.localizedDescription)"))
            return
        }
        applyEffectiveVolume()
        player.playImmediately(atRate: Float(rate))
        if state != .buffering { state = .playing }
        publishNowPlaying()
    }

    /// Pause without releasing the audio session — quicker resume.
    func pause() {
        player.pause()
        state = .paused
        publishNowPlaying()
    }

    func toggle() {
        switch state {
        case .playing, .buffering: pause()
        case .paused, .idle: play()
        case .loading, .failed: break
        }
    }

    /// Seek to absolute position in seconds.
    ///
    /// **Synchronously** updates `currentTime` to the clamped target *before*
    /// dispatching the AVPlayer seek so the rest of the app sees the new
    /// playhead immediately. The completion handler stays only to publish the
    /// Now Playing elapsed update once iOS has actually moved the player —
    /// without the eager local update, callers reading `engine.currentTime`
    /// right after `seek(to:)` would still see the pre-seek value (the
    /// completion is async on a background queue) and persist the wrong
    /// position to disk.
    func seek(to seconds: TimeInterval) {
        let target = max(0, min(seconds, duration > 0 ? duration : seconds))
        // Any user-initiated seek that lands more than 5 s before the end
        // re-arms the natural-end detection — necessary so a user who
        // finishes an episode then rewinds resumes producing position writes.
        if duration <= 0 || target < duration - 5 {
            didReachNaturalEnd = false
        }
        currentTime = target
        let time = CMTime(seconds: target, preferredTimescale: 600)
        player.seek(to: time, toleranceBefore: .zero, toleranceAfter: .zero) { [weak self] _ in
            Task { @MainActor in
                self?.publishNowPlayingElapsed()
            }
        }
    }

    /// Skip forward by `skipForwardSeconds` (default 30).
    func skip(forward seconds: TimeInterval? = nil) {
        seek(to: currentTime + (seconds ?? skipForwardSeconds))
    }

    /// Skip backward by `skipBackwardSeconds` (default 15).
    func skip(back seconds: TimeInterval? = nil) {
        seek(to: currentTime - (seconds ?? skipBackwardSeconds))
    }

    /// Variable-speed playback. 0.5–3.0 per baseline spec; clamped here.
    func setRate(_ newRate: Double) {
        let clamped = min(max(newRate, 0.5), 3.0)
        rate = clamped
        if player.timeControlStatus == .playing {
            player.rate = Float(clamped)
        }
        publishNowPlaying()
    }

    /// Arm a sleep-timer mode. See `SleepTimer.Mode`.
    func setSleepTimer(_ mode: SleepTimer.Mode) {
        sleepTimer.set(mode)
    }

    func setNowPlayingCallbacks(_ callbacks: NowPlayingCenter.Callbacks) {
        nowPlaying.setCallbacks(callbacks)
    }

    // MARK: - Now Playing wiring

    private func configureNowPlayingCallbacks() {
        var cb = NowPlayingCenter.Callbacks()
        cb.play   = { [weak self] in self?.play() }
        cb.pause  = { [weak self] in self?.pause() }
        cb.toggle = { [weak self] in self?.toggle() }
        cb.skipForward  = { [weak self] in self?.skip(forward: nil) }
        cb.skipBackward = { [weak self] in self?.skip(back: nil) }
        cb.seek         = { [weak self] t in self?.seek(to: t) }
        cb.changeRate   = { [weak self] r in self?.setRate(r) }
        // Engine-default mappings for headphone double/triple-tap. Until
        // `PlaybackState` overrides them with the user's configured action
        // (`HeadphoneGestureAction`), fall back to plain skip so a bare engine
        // still reacts to AirPods.
        cb.nextTrack     = { [weak self] in self?.skip(forward: nil) }
        cb.previousTrack = { [weak self] in self?.skip(back: nil) }
        nowPlaying.setCallbacks(cb)
    }

    private func configureSleepTimerHooks() {
        sleepTimer.onFadeTick = { [weak self] multiplier in
            guard let self else { return }
            self.sleepFadeMultiplier = multiplier
            self.applyEffectiveVolume()
        }
        sleepTimer.onFire = { [weak self] in
            guard let self else { return }
            self.onSleepTimerFire()
            self.sleepFadeMultiplier = 1.0
            self.applyEffectiveVolume()
        }
    }

    private func applyEffectiveVolume() {
        player.volume = fadeBaseVolume * sleepFadeMultiplier
    }

    // MARK: - Internal Now Playing helpers (used from +Observers extension)

    func publishNowPlaying() {
        let chapterTitle = episode.flatMap { resolveActiveChapterTitle($0, currentTime) }
        let artworkURL = episode.flatMap { resolveArtworkURL($0, currentTime) }
        if artworkURL != lastPublishedArtworkURL {
            lastPublishedArtworkImage = nil
        }
        nowPlaying.update(
            title: episode?.title,
            artist: episode.flatMap { resolveShowName($0) },
            albumTitle: chapterTitle,
            duration: duration > 0 ? duration : nil,
            elapsed: currentTime,
            rate: state == .playing ? rate : 0,
            artwork: makeMediaItemArtwork()
        )
        lastPublishedChapterTitle = chapterTitle
        // Kick off an artwork fetch when the URL changed; the result calls
        // back into `publishNowPlaying` once the image is ready so the
        // lock screen swaps in fresh artwork without us blocking publish.
        fetchArtworkIfNeeded(url: artworkURL)
    }

    /// Wrap the cached UIImage in an `MPMediaItemArtwork`. Returns `nil`
    /// when no image has resolved yet — the lock screen falls back to its
    /// default state until the fetch lands.
    ///
    /// **Concurrency.** `MPNowPlayingInfoCenter` invokes the request handler
    /// from its own internal workloop (`com.apple.MPRemoteCommandCenter`-
    /// flavoured), NOT from the main thread. The closure has to be marked
    /// `@Sendable` explicitly — without it, Swift 6 captures the enclosing
    /// `@MainActor` isolation from this class and the runtime tripwire
    /// `_swift_task_checkIsolatedSwift` traps the violation, crashing the
    /// app the moment Now Playing tries to populate the lock-screen
    /// artwork. Captured `image` is a local `let` (UIImage is Sendable
    /// since iOS 17) and `Self.resize` is `nonisolated static`, so neither
    /// capture pulls main-actor state across the boundary.
    private func makeMediaItemArtwork() -> MPMediaItemArtwork? {
        guard let image = lastPublishedArtworkImage else { return nil }
        return MPMediaItemArtwork(boundsSize: image.size) { @Sendable requested in
            // Cheap on-demand resize. iOS calls this with the exact
            // pixel bounds it needs (e.g. lock screen ~ 280pt, Control
            // Center ~ 100pt). Returning the original is fine for v1;
            // the system will down-sample without aliasing artifacts.
            if requested == image.size { return image }
            return Self.resize(image, to: requested) ?? image
        }
    }

    /// Resolve `url` via Kingfisher (shared cache) and republish nowPlaying
    /// when the resulting UIImage is ready. No-op when the URL is unchanged
    /// since the last fetch.
    private func fetchArtworkIfNeeded(url: URL?) {
        guard url != lastPublishedArtworkURL else { return }
        lastPublishedArtworkURL = url
        guard let url else {
            lastPublishedArtworkImage = nil
            return
        }
        KingfisherManager.shared.retrieveImage(with: url) { [weak self] result in
            Task { @MainActor [weak self] in
                guard let self else { return }
                // Bail when a newer fetch raced ahead — the URL we kicked
                // off may have been superseded by a chapter boundary.
                guard self.lastPublishedArtworkURL == url else { return }
                if case .success(let value) = result {
                    self.lastPublishedArtworkImage = value.image
                    // Re-publish so the new artwork lands on the lock
                    // screen. `publishNowPlaying` will re-call this fetch
                    // helper, but the URL-equality short-circuit means
                    // the second call returns immediately.
                    self.publishNowPlaying()
                }
            }
        }
    }

    /// Cheap UIGraphics resize for `MPMediaItemArtwork`'s request handler.
    /// Returns nil on failure so the caller can fall back to the original.
    ///
    /// `nonisolated` so it can be called from the `@Sendable` artwork
    /// request closure (which itself runs on `MPNowPlayingInfoCenter`'s
    /// background workloop). The inner drawing closure is also marked
    /// `@Sendable` for the same reason — without it, Swift 6 inherits
    /// MainActor isolation from the wrapping `@MainActor` class and the
    /// runtime traps when the renderer invokes the actions block off-main.
    nonisolated private static func resize(_ image: UIImage, to size: CGSize) -> UIImage? {
        let renderer = UIGraphicsImageRenderer(size: size)
        return renderer.image { @Sendable _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
    }

    func publishNowPlayingElapsed() {
        nowPlaying.updateElapsed(currentTime, rate: state == .playing ? rate : 0)
    }

    // MARK: - State setter (used from +Observers extension)

    /// Exposed for the observer extension; internal so call-sites stay tidy.
    func setState(_ newState: State) {
        self.state = newState
    }

    func setDuration(_ newDuration: TimeInterval) {
        self.duration = newDuration
    }

    func setCurrentTime(_ newTime: TimeInterval) {
        self.currentTime = newTime
    }
}
