# Lane 4 — Player UI + Persistent Mini-Bar

> Now Playing surface and persistent mini-player. UI built against a
> `MockPlaybackState` so Lane 1 can later swap in the real `AudioEngine`
> without surface changes.

## Files

All new files under `App/Sources/Features/Player/`:

| File                                | LoC | Purpose                                                                 |
|-------------------------------------|-----|-------------------------------------------------------------------------|
| `MockPlaybackState.swift`           | 250 | `@MainActor @Observable` model — the binding contract for Lane 1.       |
| `MockTranscriptFixture.swift`       | 109 | Static demo data: 40-line two-speaker keto interview, palette baked in. |
| `PlayerView.swift`                  | 289 | Full-screen Now Playing surface (hero + transcript + waveform + chrome).|
| `MiniPlayerView.swift`              | 172 | Persistent dock above tab bar; **shows active transcript line**.        |
| `PlayerControlsView.swift`          | 125 | Transport row + secondary glass action cluster.                         |
| `PlayerWaveformView.swift`          | 112 | `Canvas`-driven amplitude waveform with semantic speaker stripes.       |
| `PlayerTranscriptScrollView.swift`  | 155 | Auto-scrolling transcript, speaker chips, tap-to-jump, long-press hook. |
| `PlayerSpeedSheet.swift`            | 63  | Speed picker bottom sheet.                                              |
| `PlayerSleepTimerSheet.swift`       | 71  | Sleep-timer presets bottom sheet.                                       |
| `PlayerTimeFormat.swift`            | 27  | Shared `mm:ss` / `h:mm:ss` formatter (tabular numerals never jitter).   |

Every file under the 300-line soft limit; total lane footprint ≈ 1,373 LoC.

## `MockPlaybackState` shape — Lane 1 binding contract

The view layer depends on this surface. When Lane 1 ships the real
`AudioEngine`, point `RootView`'s `@State` at it (or wrap it in a small
adapter) and the views recompile unchanged.

```swift
@MainActor @Observable
final class MockPlaybackState {
    // Observable state -----------------------------------------
    var isPlaying: Bool
    var currentTime: TimeInterval
    var rate: MockPlaybackRate          // 0.8 / 1.0 / 1.2 / 1.5 / 2.0
    var sleepTimer: MockSleepTimer
    var isAirPlayActive: Bool
    var episode: MockEpisode?           // nil → no mini-bar
    var transcript: [MockTranscriptLine]

    // Derived ---------------------------------------------------
    var duration: TimeInterval { episode?.duration ?? 0 }
    var activeLineIndex: Int?
    var activeLine: MockTranscriptLine?

    // Imperative API (the contract Lane 1 must implement) -------
    func togglePlayPause()
    func play()
    func pause()
    func seek(to time: TimeInterval)
    func seekSnapping(to time: TimeInterval) // ±400ms sentence snap
    func skipBackward(_ seconds: TimeInterval = 15)
    func skipForward(_ seconds: TimeInterval = 30)
    func setRate(_ rate: MockPlaybackRate)
    func setSleepTimer(_ timer: MockSleepTimer)
    func jumpToLine(_ line: MockTranscriptLine)
}
```

Companion lane-local types:

- `MockEpisode` — `id`, `showName`, `episodeNumber`, `title`, `chapterTitle`,
  `duration`, `primaryArtColor`, `secondaryArtColor`. Lane 2's real `Episode`
  must surface at least the metadata fields; cover-art extraction (Lane 1's
  `UIImage.dominantColors`) supplies the two palette colors.
- `MockTranscriptLine` — `id`, `speakerID`, `speakerName`, `speakerColor`,
  `text`, `start`, `end`. Lane 5's real transcript stream needs to publish
  identical fields (or an adapter at the view boundary).
- `MockPlaybackRate` enum — five canonical rates, used by the speed sheet.
- `MockSleepTimer` enum — `.off`, `.minutes(Int)`, `.endOfEpisode`.

The mock drives `currentTime` forward via a `@MainActor` `Task` ticking every
~120ms; Lane 1 will replace this with `AVPlayer.addPeriodicTimeObserver`. The
ticker honours `rate.rawValue`, so changing speed in the UI is observably
correct against the demo timer.

## `RootView.swift` — what changed

Single careful edit, scoped to the top of `body` so the rest of the file is
unchanged. Diff summary:

1. Added three properties to `RootView`:
   - `@State private var mockPlaybackState = MockPlaybackState()`
   - `@State private var showFullPlayer = false`
   - `@Namespace private var playerNamespace` (shared geometry between
     mini-bar and full player).
2. Inserted at the top of the modifier chain on `body`:
   - `.environment(mockPlaybackState)` — makes the state available to any
     downstream view that wants to query the player.
   - `.safeAreaInset(edge: .bottom, spacing: 0) { … MiniPlayerView … }` —
     gates on `mockPlaybackState.episode != nil` so the bar appears only
     when something is loaded. Tapping the bar sets `showFullPlayer = true`.
   - `.fullScreenCover(isPresented: $showFullPlayer) { PlayerView(...) }` —
     full Now Playing surface, sharing `playerNamespace` for matched
     geometry on artwork / play glyph / surface.
3. Existing `.onShake` → `.fullScreenCover` (onboarding) → deep-link chain
   is **untouched** below my insert; tab structure and routing are intact.

The mini-bar appears immediately on first launch because `MockPlaybackState`
loads a demo episode in `init()`. Lane 1's real engine will instead start
with `episode = nil` and the mini-bar will not render until the user starts
playback.

## Microinteractions implemented vs deferred

| UX-01 §5 microinteraction                                         | Status      |
|-------------------------------------------------------------------|-------------|
| Mini-bar → full player matched geometry (artwork, play glyph)     | Implemented (shared `glassEffectID` + `Namespace`) |
| Active transcript line lights + size step (220ms ease-out feel)   | Implemented (spring on `activeLine.id`) |
| Auto-scroll to active line                                        | Implemented (`ScrollViewReader.scrollTo(.center)`) |
| "Return to live" pill when user scrolls manually                  | Scaffolded (state present, manual-scroll detection is heuristic only — full ScrollOffset reading deferred to Lane 5 transcript reader) |
| Scrubber engaged → artwork blur + 1.04 scale + waveform 56→220pt  | Implemented |
| Scrub release snap to nearest sentence boundary (±400ms)          | Implemented (`seekSnapping`) |
| Hold-to-clip on transcript line (600ms long-press, haptic ramp)   | Wired to gesture; clip sheet itself defers to Lane 5 (placeholder) |
| Speed change via long-press play (radial dial)                    | Deferred — speed sheet covers the case; radial dial is a Lane-4 follow-up |
| Tap mini-bar → expand                                             | Implemented |
| Active transcript line as mini-bar ticker                         | **Implemented — the lane signature** |
| Play/pause haptics                                                | Implemented (`Haptics.medium` / `Haptics.soft`) |
| Speaker chip with color dot + mono name                           | Implemented |
| Semantic waveform stripes (per-speaker)                           | Implemented (visible while scrubbing per brief) |
| Agent chip + inline answer card                                   | Deferred to Lane 10 (not Lane 4 territory) |
| Resume pill ("24:18 — 'and that's where the protocol changes'")   | Deferred (needs cross-launch state from Lane 1) |
| Chapter cross sweep                                               | Deferred (chapter model lives in Lane 2) |
| Bookmark add                                                      | Deferred (bookmark store TBD) |

## Boundary respected

- No edits under `App/Sources/Audio/`, `App/Sources/Podcast/`,
  `App/Sources/Features/Library/`, `App/Sources/Features/EpisodeDetail/`,
  `App/Sources/Transcript/`, `App/Sources/Knowledge/`,
  `App/Sources/Voice/`, `App/Sources/Briefing/`.
- `Project.swift` and `App/Resources/Info.plist` untouched.
- `App/Sources/App/RootView.swift` modified minimally as described above.
- `MockEpisode` is intentionally lane-local (no dependency on Lane 2).
- No SPM dependencies added.

## Build status

`xcodebuild -workspace AppTemplate.xcworkspace -scheme AppTemplate
-destination "generic/platform=iOS Simulator" build` →
**BUILD SUCCEEDED**, no errors, no Lane-4-attributable warnings.

## Lane 1 swap-in checklist

1. Replace `@State private var mockPlaybackState = MockPlaybackState()` in
   `RootView` with the real engine instance (or an adapter conforming to the
   same observable surface).
2. Update `MockEpisode` consumers to read from Lane 2's `Episode` (only the
   six fields listed above are read).
3. Replace `MockTranscriptFixture.timFerrissKetoDemo` with Lane 5's
   transcript stream; honour the same `MockTranscriptLine` field names or
   provide a mapper at the view boundary.
4. Remove `MockPlaybackState.startDemoTimer()` — `AVPlayer`'s periodic time
   observer becomes the source of truth for `currentTime`.
5. The five `Mock*` types and the demo timer are the *only* throwaway
   surface in this lane. Everything else is production code.
