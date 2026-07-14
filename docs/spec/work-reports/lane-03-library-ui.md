# Lane 3 — Library UI

> Worktree: `worktree-agent-a917090ff93e95ca7`
> Owner: Lane 3 (Library UI)
> Status: complete, build green, one commit pending.

## Summary

Replaces the placeholder `Features/Library/LibraryView.swift` with a real,
polished UI driven by an in-memory mock store. Subscriptions grid +
filter chips + search-entry deep-link on the Library tab; show detail
with artwork-tinted header, episode list, "Settings for this show"
sheet, and OPML import sheet. Designed so Lane 2 can drop in real data
via a single typealias rename.

## Files written

All under `App/Sources/Features/Library/`. File-size budget respected
(soft 300 / hard 500); largest file is `LibraryMockStoreSeed.swift` at
256 lines.

| File                            | Lines | Role                                                                    |
|---------------------------------|------:|-------------------------------------------------------------------------|
| `LibraryView.swift`             |   201 | Tab root. Search-bar + filter rail + subscriptions grid + OPML sheet.   |
| `LibraryFilters.swift`          |   113 | `LibraryFilter` enum + `LibraryFilterChip` + `LibraryFilterRail`.       |
| `LibraryGridCell.swift`         |    92 | Matte subscription card (artwork tile + title + author + unplayed dot). |
| `LibraryMockStore.swift`        |   197 | `@Observable` mock store + shim models + filter/import API.             |
| `LibraryMockStoreSeed.swift`    |   256 | Static seed: 10 subscriptions × 4 episodes = 40 episodes (>30 req).    |
| `ShowDetailView.swift`          |   218 | Show detail screen — header + description + chip rail + episode list.   |
| `ShowDetailHeader.swift`        |   147 | Artwork-tinted hero — gradient → background fade, hero artwork tile.    |
| `EpisodeRow.swift`              |   132 | State-aware row (unplayed dot / crescent / check / capsule).            |
| `EpisodeDetailLink.swift`       |    71 | Tap-to-push helper + `EpisodeDetailViewStub` (Lane 5 merge seam).       |
| `OPMLImportSheet.swift`         |   252 | Three-phase sheet: pick → review → progress. Glass surface.             |
| `DownloadStatusCapsule.swift`   |   127 | Reusable capsule for download/transcription state.                      |

## Mock store shape

The store holds two collections behind a `@MainActor @Observable`
class. **Both shim types live inside `LibraryMockStore.swift`** so
Lane 2 sees a single file to reconcile.

```swift
struct LibraryMockSubscription: Identifiable, Hashable {
    let id: UUID
    let title: String                // "The Tim Ferriss Show"
    let author: String               // "Tim Ferriss"
    let artworkSymbol: String        // SF Symbol stand-in for art
    let accentHue: Double            // 0...1 — drives detail tint
    let episodeCount: Int
    let unplayedCount: Int
    let isSubscribed: Bool
    let wikiReady: Bool
    let transcriptsEnabled: Bool
    let showDescription: String

    var accentColor: Color { /* HSB from accentHue */ }
    var hasUnplayed: Bool  { unplayedCount > 0 }
}

struct LibraryMockEpisode: Identifiable, Hashable {
    let id: UUID
    let subscriptionID: UUID
    let number: Int
    let title: String
    let summary: String
    let durationSeconds: Int
    let publishedAt: Date
    let isPlayed: Bool
    let playbackProgress: Double     // 0...1
    let downloadStatus: DownloadStatus

    var isInProgress: Bool { /* derived */ }
    var isUnplayed: Bool   { /* derived */ }
    var formattedDuration: String { /* "2h 14m" */ }
}
```

`DownloadStatus` is the public per-episode state surface:

```swift
enum DownloadStatus: Equatable, Hashable {
    case none
    case downloaded(transcribed: Bool)
    case downloading(progress: Double)
    case transcribing(progress: Double)
    case transcriptionQueued(position: Int)
    case failed
}
```

## Deep-link conventions

### Library → Ask tab (search bar)

`LibraryView` accepts an `onOpenSearch: () -> Void` closure. Defaults
to a haptic-only no-op so the view is constructible in any container
(SwiftUI previews, tests, or while the Ask tab doesn't exist yet).

The orchestrator wires this at merge:

```swift
// In RootView, when adding the library tab:
LibraryView(onOpenSearch: { selectedTab = .ask })
```

### Show detail → episode detail (Lane 5)

Tapping an episode pushes a `LibraryEpisodeRoute` value onto
`ShowDetailView`'s enclosing `NavigationStack`:

```swift
struct LibraryEpisodeRoute: Hashable {
    let episodeID: UUID
    let subscriptionID: UUID
    let title: String
}
```

`ShowDetailView.body` resolves the route through
`navigationDestination(for: LibraryEpisodeRoute.self)` to
`EpisodeDetailViewStub` (declared in `EpisodeDetailLink.swift`). At
merge, **Lane 5 swaps the resolver body to its real
`EpisodeDetailView(route:)`**; the stub file can be removed once
nothing references it. The route signature is the contract.

## Glass placement

Strict adherence to the lane brief — "structural glass on the nav
chrome and the OPML import sheet only; cards are matte."

| Surface                                            | Glass? | Tier            |
|----------------------------------------------------|:------:|------------------|
| Library: search-entry capsule                      |  yes   | T1 clear         |
| Library: filter rail container                     |  yes   | T1 clear         |
| ShowDetail: filter rail container                  |  yes   | T1 clear         |
| OPML sheet: container                              |  yes   | `.ultraThinMaterial` |
| ShowDetail settings sheet: container               |  yes   | `.thinMaterial`  |
| `DownloadStatusCapsule`                            |  yes   | T2 tinted (status-keyed) |
| Subscription cards (`LibraryGridCell`)             |  no    | matte            |
| Episode rows (`EpisodeRow`)                        |  no    | matte            |
| `ShowDetailHeader`                                 |  no    | matte (gradient) |

## What Lane 2 needs to know to swap mocks for real data

1. **Type rename = single mechanical pass.** The two shim types
   `LibraryMockSubscription` / `LibraryMockEpisode` live exclusively in
   `LibraryMockStore.swift`. Lane 2's real model probably uses
   `Subscription` / `Episode`. Two strategies the orchestrator can pick:

   - *Typealias bridge* (smallest diff, recommended for first merge):
     ```swift
     typealias LibraryMockSubscription = Subscription
     typealias LibraryMockEpisode      = Episode
     ```
     followed by adding any missing computed helpers
     (`hasUnplayed`, `formattedDuration`, `isInProgress`, etc.) as
     extensions on the real types if Lane 2's model doesn't already
     provide them.

   - *Direct rename*: search-and-replace
     `LibraryMockSubscription → Subscription` (and `…Episode`) across
     the eight Lane 3 files.

2. **`accentHue` / `accentColor` is artwork-derived.** Lane 3 hard-codes
   per-show hue values in the seed. The real implementation should
   compute the accent color by sampling the dominant color of the
   subscription's artwork (e.g. `UIImage.dominantColor` via
   `CIAreaAverage` or a k-means swatch extractor) and store it
   alongside the subscription model. The header gradient and the
   "in-progress crescent" both consume it.

3. **`artworkSymbol` is the SF-Symbol stand-in.** Lane 2's real model
   replaces this with whatever art identifier you use (`URL`, cached
   `UIImage`, or a `Kingfisher`-style key). Two views consume it:
   - `LibraryGridCell.artworkTile` — 1:1 tile in the grid.
   - `ShowDetailHeader.artwork` — 220pt tile in the detail header.

4. **`LibraryMockStore` is the dependency-injection seam.** Three Lane
   3 views currently take `let store: LibraryMockStore`:
   - `LibraryView` (constructs one internally; can be made injectable
     by changing the property to `@State` + an `init(store:)`).
   - `ShowDetailView`
   - `OPMLImportSheet`

   Lane 2 should either:
   - rename the type (see #1) and keep prop-passing, or
   - promote a real podcast-store into `AppStateStore` (existing
     pattern in this codebase via `@Environment(AppStateStore.self)`)
     and refactor these three init signatures to read from the
     environment instead.

5. **OPML parsing is mocked.** `LibraryMockStore.importMockOPML` and
   `OPMLImportSheet.runImport` simulate progress and append
   `LibraryMockStoreSeed.makeImported(index:)` rows. Lane 2 should
   replace the body of those two methods with real OPML parsing +
   `Task { for await ... in subscribeStream { ... } }`. The
   three-phase sheet UI (`pick / review / progress`) and the
   `OPMLImportPhase` enum can stay as-is.

6. **`EpisodeDetailViewStub` is Lane 5's merge target.** Don't remove
   it from Lane 3 — Lane 5 deletes the stub when their real view
   lands.

7. **`DownloadStatus` may not match Lane 2's state model exactly.** The
   six cases here are intentionally close to the spec wireframes
   (ux-02 §6.E). If Lane 2's `EpisodeState` uses different splits
   (e.g. "queued" subdivided), add a small mapping initializer:
   `init(_ state: EpisodeState)` on `DownloadStatus`.

## Build status

```
$ tuist generate --no-open
✔ Project generated.

$ xcodebuild -workspace AppTemplate.xcworkspace -scheme AppTemplate \
    -destination 'generic/platform=iOS Simulator' \
    -configuration Debug build CODE_SIGNING_ALLOWED=NO
…
** BUILD SUCCEEDED **
```

Zero new warnings or errors in the eleven Lane 3 files. Pre-existing
warnings in unrelated files (Settings, Services, AVFoundation
@Sendable, deprecated SKStoreReviewController) are untouched and out
of scope.

## Notes for the orchestrator

- `RootView` was **not modified** per the lane brief; the Library tab
  is not yet wired into the tab bar. The orchestrator adds the tab in
  the integration lane.
- The placeholder `LibraryView.swift` stub from main is replaced by
  the real implementation at `App/Sources/Features/Library/LibraryView.swift`.
- All eight files prescribed by the lane brief are present, plus
  three additional files split out to honor the 300-line soft limit:
  `LibraryMockStoreSeed.swift`, `LibraryGridCell.swift`,
  `ShowDetailHeader.swift`. These are pure code-organization splits;
  the orchestrator does not need to reconcile them.
