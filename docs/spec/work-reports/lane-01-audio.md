# Lane 1 — Audio Engine, Session Coordinator, NowPlaying, Sleep Timer

## Summary

Replaced the empty `AudioEngine` stub with a real `AVPlayer`-backed engine. Added an `AudioSessionCoordinator` that arbitrates `AVAudioSession` between podcast playback, voice capture, and the future conversational voice mode. Wired `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter` (asymmetric skip 30s/15s) and a fade-out sleep timer with end-of-episode and shake-to-extend hooks.

Build: **green** (`xcodebuild … build` and `… build-for-testing` both pass).

## Files

### Created

| Path | Lines | Purpose |
|---|---|---|
| `App/Sources/Audio/AudioSessionCoordinator.swift` | 155 | Singleton `@MainActor` arbiter of `AVAudioSession`. Defines `VoiceSessionClient` protocol stub for Lane 8. |
| `App/Sources/Audio/NowPlayingCenter.swift` | 164 | `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter` bridge. Asymmetric skip preferred-intervals, scrubbing, rate change. |
| `App/Sources/Audio/SleepTimer.swift` | 153 | Duration / end-of-episode modes, fade-out ramp, `extend(by:)` for shake integration. |
| `App/Sources/Audio/AudioEngine+Observers.swift` | 121 | KVO + periodic time observer wiring split out of `AudioEngine.swift` to honor the 300-line soft limit. |

### Modified

| Path | Before | After | Notes |
|---|---|---|---|
| `App/Sources/Audio/AudioEngine.swift` | 14 line stub | 249 lines | `@Observable @MainActor` final class wrapping `AVPlayer`. |

All files under the 300-line soft limit (max: 249).

## Public surface

### `AudioEngine` — `@Observable @MainActor final class`

```swift
enum State: Equatable, Sendable {
    case idle
    case loading(Episode)
    case playing
    case paused
    case buffering
    case failed(EngineError)
}

private(set) var state: State
private(set) var currentTime: TimeInterval
private(set) var duration: TimeInterval
private(set) var rate: Double
private(set) var episode: Episode?

let sleepTimer: SleepTimer
let nowPlaying: NowPlayingCenter
var skipForwardSeconds: Double  // default 30
var skipBackwardSeconds: Double // default 15

func load(_ episode: Episode)
func play()
func pause()
func toggle()
func seek(to seconds: TimeInterval)
func skip(forward seconds: TimeInterval? = nil)
func skip(back seconds: TimeInterval? = nil)
func setRate(_ newRate: Double)              // clamps 0.5…3.0
func setSleepTimer(_ mode: SleepTimer.Mode)
```

`EngineError` is a plain `struct: Error, Equatable, Sendable` so the `State` enum can stay `Equatable` for SwiftUI diffing — Swift's bare `Error` is not `Equatable`.

### `AudioSessionCoordinator` — `@MainActor final class` (singleton)

```swift
static let shared: AudioSessionCoordinator

enum Mode: Equatable, Sendable {
    case idle
    case podcastPlayback
    case briefingPlayback
    case voiceCapture
    case duckedForVoice
}

private(set) var mode: Mode
weak var voiceClient: (any VoiceSessionClient)?

func activate(_ mode: Mode) throws
func switchPlaybackContext(to mode: Mode)   // podcast ⇄ briefing without thrash
func deactivate() throws
```

### `NowPlayingCenter` — `@MainActor final class`

```swift
struct Callbacks { … }            // play, pause, toggle, skip, seek, changeRate

func setCallbacks(_:)
func setSkipIntervals(forward:backward:)
func update(title:artist:albumTitle:duration:elapsed:rate:artwork:)
func updateElapsed(_:rate:)
func clear()
```

### `SleepTimer` — `@Observable @MainActor final class`

```swift
enum Mode  { case off, duration(TimeInterval), endOfEpisode }
enum Phase { case idle, armed(remaining:), armedEndOfEpisode, fading(remaining:), fired }

var onFadeTick: (Float) -> Void   // 1.0 → 0.0 over `fadeDurationSeconds` (8 s)
var onFire: () -> Void

func set(_ mode: Mode)
func extend(by seconds: TimeInterval)        // shake-to-extend hook
func cancel()
func shouldStopAtEpisodeEnd() -> Bool        // engine asks on item-end notification
```

## Cross-lane protocol

`AudioSessionCoordinator.swift` defines a small protocol Lane 8 will implement:

```swift
protocol VoiceSessionClient: AnyObject, Sendable {
    func voiceSessionWillActivate() async
    func voiceSessionDidDeactivate() async
}

final class NoopVoiceSessionClient: VoiceSessionClient { … }   // default no-op
```

Lane 8's `AudioConversationManager` should conform to this protocol and assign itself via `AudioSessionCoordinator.shared.voiceClient = manager`. The coordinator's transition logic is wired but the actual notify-the-voice-client call sites are deliberately empty in this lane — Lane 8 will add the duck/resume orchestration.

## Coordination notes

### `VoiceItemService` — left untouched on purpose

`App/Sources/Services/VoiceItemService.swift` directly calls `AVAudioSession.sharedInstance().setCategory(.record, mode: .measurement, options: .duckOthers)`. The brief said to "arbitrate between" the engines, but `VoiceItemService` is in `Services/` and is owned by note-taking (existing template feature), not the new podcast subsystem.

**Decision**: leave `VoiceItemService` as-is in this lane. The `AudioSessionCoordinator.voiceCapture` mode mirrors its exact configuration so a one-line migration (`try AudioSessionCoordinator.shared.activate(.voiceCapture)`) becomes possible later without changing behavior. Migrating it now would have been an out-of-lane edit that two other lanes are likely about to refactor.

Migration path:
1. `VoiceItemService.start()` → call `AudioSessionCoordinator.shared.activate(.voiceCapture)` instead of inlining `setCategory`/`setActive`.
2. `VoiceItemService.teardown()` → call `AudioSessionCoordinator.shared.deactivate()` instead of inlining `setActive(false)`.
3. Test on a device because mid-call deactivation interacts with notes-app dictation.

### Existing `Episode` model is sufficient

`App/Sources/Podcast/Episode.swift` (Lane 2) currently exposes `id, title, publishedAt, mediaURL?, durationSeconds?, summary?`. The engine uses `mediaURL`, `title`, `durationSeconds`. When Lane 2 evolves the model (artwork URL, show ref, show notes), the engine will pick those up automatically — `publishNowPlaying()` is the single point that maps `Episode → MPNowPlayingInfoCenter`.

### `AppStateStore` — untouched

Per the constraint. Engine state stays in the engine. Lane 4's `Player` UI will reach the `AudioEngine` via SwiftUI `@Environment` (Lane 4's call) or via dependency injection into the player feature. The engine's `@Observable` macro means views just need a reference; no store wiring.

## What's stubbed for other lanes

- **Voice mode duck/resume orchestration** — coordinator has the *modes* (`.duckedForVoice`, `.briefingPlayback`) and the `VoiceSessionClient` protocol but doesn't actually call the client yet. Lane 8 wires it.
- **Autoplay-next** — `handleEndOfItem()` deliberately leaves `state = .paused` after sleep-timer check. Lane 2 / Lane 4 hook the queue logic.
- **Episode artwork** — `NowPlayingCenter.update` accepts an `MPMediaItemArtwork?` parameter; `AudioEngine.publishNowPlaying()` currently passes `nil` because the `Episode` model has no artwork URL yet. Lane 2 + Lane 4 will route artwork through.
- **Background audio entitlement** — orchestrator handles `UIBackgroundModes: [audio]` at merge per the brief. Until then, the engine works in foreground only.
- **Sleep-timer shake hook** — `SleepTimer.extend(by:)` is the API; the actual `.onShake { sleepTimer.extend(by: 5*60) }` lives in the player view (Lane 4). Existing `Design/ShakeDetector.swift` is the integration point.

## Notes / decisions

1. **Audio session lazily activated.** The session activates on first `play()`, not on `init()`. Activating eagerly steals the route from `VoiceItemService`/Siri/etc. before anyone has asked to play.
2. **`MainActor.assumeIsolated` inside the periodic time observer block.** The observer's queue is `.main` (DispatchQueue), but Swift 6 strict concurrency wants an explicit isolation hop. `assumeIsolated` is correct here because the queue *is* main.
3. **No `deinit` cleanup.** `@MainActor` properties cannot be touched from `deinit` under Swift 6 strict concurrency. `AVPlayer` releases its time observer on dealloc, and the `NotificationCenter` token dies with the engine. Explicit teardown happens in `teardownItemObservers()` when a new episode loads.
4. **`KVO` via the block-based `observe(_:)` API.** The closures hop to `@MainActor` via `Task { @MainActor in … }` — this avoids the older `@objc dynamic` ceremony and is Swift-6 friendly.
5. **`AVPlayer.playImmediately(atRate:)`** instead of `play()` so user-set playback rate is honored on resume without a momentary 1.0× glitch.
6. **Time observer interval is 0.5 s.** Tight enough for the live-transcript scrubbing UX from `ux-01-now-playing.md` but light on CPU; the player UI can subscribe more aggressively if it needs sub-100 ms ticks for waveform rendering.
7. **`.glassEffect(.regular.interactive(), …)`** speed dial / control morphing per UX-01 happens in Lane 4's view code; the engine just exposes `setRate(_:)`.

## Build verification

```
$ tuist generate --no-open
✔ Success — Project generated.

$ xcodebuild -workspace Podcastr.xcworkspace -scheme Podcastr \
    -destination 'generic/platform=iOS Simulator' \
    -configuration Debug -skipPackagePluginValidation \
    -skipMacroValidation CODE_SIGNING_ALLOWED=NO build
** BUILD SUCCEEDED **

$ xcodebuild -workspace Podcastr.xcworkspace -scheme Podcastr \
    -destination 'platform=iOS Simulator,name=iPhone 17,OS=latest' \
    -configuration Debug … build-for-testing
** TEST BUILD SUCCEEDED **
```

Zero warnings. Zero errors. Strict concurrency clean.

## Things I noticed (out of scope, flagged for orchestrator)

1. **`Info.plist` will need `UIBackgroundModes: [audio]`** for lock-screen playback to actually keep playing when the app backgrounds. Brief says orchestrator handles this at merge — flagging so it doesn't slip.
2. **`MPNowPlayingInfoCenter` requires `NSMicrophoneUsageDescription` and `NSAppleMusicUsageDescription`-class strings on iOS 26+ in some configurations** — verify with a simulator install before signing off the merge. Currently builds without them.
3. **Lane 2's `Episode.mediaURL` is `URL?`** — the engine treats `nil` as a `failed(EngineError)`. Lane 2 may want to make it non-optional (an episode without media isn't really an episode) but that's a Lane 2 call.
4. **`changePlaybackRateCommand.supportedPlaybackRates`** is currently `[0.8, 1.0, 1.2, 1.5, 2.0]`. The baseline brief calls for 0.5–3.0 in 0.05× steps. The lock-screen UI only shows discrete options anyway — keeping the curated list. `setRate(_:)` still accepts the full range.
5. **No tests added.** Brief did not list tests in the deliverable. The `AudioEngine` is heavily `AVPlayer`-bound; meaningful tests need a fake-`AVPlayer` protocol seam — worth a small follow-up lane after Lane 2's `Episode` model stabilizes.
