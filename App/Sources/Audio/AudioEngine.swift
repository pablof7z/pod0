import AVFoundation
import Combine
import Foundation
import os.log
import UIKit
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
    var onFailure: (ProductFailure) -> Void = { _ in }
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

    /// Called when `SleepTimer` fires.
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
    var audioSessionObserver: PlaybackAudioSessionObserver?
    var fadeBaseVolume: Float = 1.0
    var pendingInitialSeekTime: TimeInterval?
    var playRequested = false

    /// Typed raw lifecycle observations for the Rust playback-policy owner.
    var onHostStateChanged: () -> Void = { }
    var onHostAudioSessionEvent: (PlaybackAudioSessionEvent) -> Void = { _ in }
    var onPresentationTimeChanged: (TimeInterval) -> Void = { _ in }

    /// Per-effect multiplier that composes into `player.volume` via
    /// `applyEffectiveVolume`. Sleep timer drives `sleepFadeMultiplier`.
    var sleepFadeMultiplier: Float = 1.0

    // MARK: - Init / deinit

    init() {
        configureNowPlayingCallbacks()
        onSleepTimerFire = { }
        configureSleepTimerHooks()
        nowPlaying.setSkipIntervals(forward: skipForwardSeconds, backward: skipBackwardSeconds)
        configureAudioSessionObserver()
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
    /// Prefers the verified Rust-projected local artifact when available.
    func load(
        _ episode: Episode,
        requestedURL: URL? = nil,
        initialPosition: TimeInterval = 0
    ) {
        let url: URL = {
            if let local = episode.downloadState.localFileURL,
               FileManager.default.fileExists(atPath: local.path) {
                return local
            }
            // Agent-generated episodes live under agent-episodes/, not downloads/.
            // Recompute the path fresh so stale container-relative absolute URLs
            // (persisted from a previous launch) are never handed to AVPlayer.
            if episode.generationSource != nil,
               episode.enclosureURL.isFileURL,
               let freshURL = try? CoreAgentGeneratedAudioFileStore.currentURL(
                   for: episode.enclosureURL
               ),
               FileManager.default.fileExists(atPath: freshURL.path) {
                return freshURL
            }
            return requestedURL ?? episode.enclosureURL
        }()
        teardownItemObservers()
        self.episode = episode
        playRequested = false
        let feedDuration = episode.duration ?? 0
        let boundedInitialPosition = max(
            0,
            min(initialPosition, feedDuration > 0 ? feedDuration : initialPosition)
        )
        pendingInitialSeekTime = boundedInitialPosition > 0 ? boundedInitialPosition : nil
        setState(.loading(episode))
        setCurrentTime(boundedInitialPosition)
        setDuration(feedDuration)
        didReachNaturalEnd = false

        let asset = AVURLAsset(
            url: url,
            options: Self.assetOptions(for: episode, sourceURL: url)
        )
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

    // MARK: - Observable state setters

    func setState(_ newState: State) {
        state = newState
        if case let .failed(error) = newState { onFailure(error.failure) }
        onHostStateChanged()
    }

    func setDuration(_ newDuration: TimeInterval) {
        duration = newDuration
        onHostStateChanged()
    }

    func setCurrentTime(_ newTime: TimeInterval) {
        currentTime = newTime
        onPresentationTimeChanged(newTime)
        onHostStateChanged()
    }

    func setPlaybackRate(_ newRate: Double) {
        rate = newRate
        onHostStateChanged()
    }

    static func assetOptions(for episode: Episode, sourceURL: URL) -> [String: Any] {
        guard sourceURL.isFileURL, let enclosureMimeType = episode.enclosureMimeType else {
            return [:]
        }
        // Core download artifacts intentionally use opaque `.media`
        // filenames. Preserve that durable storage contract while giving
        // AVFoundation the feed-declared format it cannot infer from the
        // local path extension.
        return [AVURLAssetOverrideMIMETypeKey: enclosureMimeType]
    }

}
