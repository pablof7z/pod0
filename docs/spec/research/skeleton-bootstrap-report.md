# Skeleton Bootstrap Report — Podcastr

> Engineer agent working in worktree `worktree-agent-aa86a44a8641c7e16` at
> `/Users/pablofernandez/Work/podcast-player/.claude/worktrees/agent-aa86a44a8641c7e16/`.
> A single commit on that branch carries the work.

## Summary

The cloned `ios-app-template` has been renamed to **Podcastr** end-to-end and a
clean module/feature scaffold has been added. The project regenerates with
Tuist and builds clean for the iOS simulator. No new SPM dependencies were
introduced; SwiftData was not touched; no audio code was written. All work is
non-functional placeholders awaiting the synthesized product spec.

## Rename surface

### What the task explicitly requested
- `Project.swift` — `appName = "Podcastr"`, `appDisplayName = "Podcastr"`,
  `bundleIdPrefix = "com.podcastr"`. `appleTeamID` left at `456SHKPP26` per
  instruction.
- App Group hardcoded to `group.com.podcastr.app` (does NOT follow the
  bundle-ID derivation pattern — the previous derivation
  `group.\(appBundleID)` would have produced `group.com.podcastr.podcastr`).
  Rationale: future-proof against the working title changing without
  re-provisioning the App Group on Apple's side.
- Entitlements files renamed via `git mv`:
  - `App/Resources/AppTemplate.entitlements` → `Podcastr.entitlements`
  - `App/Widget/Resources/AppTemplateWidget.entitlements` → `PodcastrWidget.entitlements`
- `App/Sources/State/Persistence.swift` — App Group fallback +
  `stateKey = "podcastr.state.v1"`.
- `.github/workflows/testflight.yml` — `APP_SCHEME`, `PROJECT_PATH`,
  `APP_BUNDLE_ID`. `APPLE_TEAM_ID` left as `XXXXXXXXXX` placeholder per
  instruction.
- `README.md` and `CLAUDE.md` — README header rewritten; pointers to AGENTS.md
  / PROJECT_CONTEXT.md added; remaining template setup instructions kept and
  updated to use `Podcastr.xcodeproj`. `CLAUDE.md` is generic and was left
  alone.

### Additional rename surface I had to handle (not in the task list)
- `App/Widget/Sources/WidgetBundle.swift` — `AppTemplateWidgetBundle` →
  `PodcastrWidgetBundle`.
- `App/Widget/Sources/WidgetModels.swift` — App Group fallback + `stateKey`.
- `App/Sources/AppMain.swift` — `AppTemplateApp` → `PodcastrApp`.
- `App/Sources/Intents/AppTemplateShortcuts.swift` — file renamed (`git mv`)
  to `PodcastrShortcuts.swift`; struct renamed to `PodcastrShortcuts`.
- `App/Sources/Intents/AddItemIntent.swift` — doc-comment cross-reference.
- `App/Sources/Intents/FocusFilterIntent.swift` — UserDefaults keys
  `apptemplate.focus.{tag,choice}` → `podcastr.focus.{tag,choice}`.
- `App/Sources/Services/DeepLinkHandler.swift` — URL scheme `apptemplate://`
  → `podcastr://` (resolve and builder).
- `App/Sources/App/AppDelegate.swift` — quick-action URL strings.
- `App/Sources/Services/HandoffActivityType.swift` — activity type identifier.
- `App/Sources/Services/SpotlightIndexer.swift` — three Spotlight domain
  identifiers (`com.apptemplate.spotlight.{items,notes,memories}` →
  `com.podcastr.spotlight.*`).
- `App/Sources/Services/{Nostr,OpenRouter,ElevenLabs}CredentialStore.swift`
  and `UserIdentityStore.swift` — keychain service-name fallback strings
  (these compose with `Bundle.main.bundleIdentifier`, so the live value
  changes automatically; only the fallback was stale).
- `App/Sources/Design/AppLogger.swift` — `os.Logger` subsystem fallback.
- `App/Sources/Services/DataExport.swift` — `AppTemplate-Export-*.json` and
  `AppTemplate-Items-*.csv` filename prefixes.
- `App/Sources/State/AppStateStore.swift` — doc-comment scheme reference.
- `App/Resources/Info.plist` — `CFBundleURLSchemes` (`apptemplate` →
  `podcastr`); `NSUserActivityTypes` editItem identifier.
- `AppTests/Sources/AppTests.swift` — `@testable import AppTemplate` →
  `@testable import Podcastr`; expected export-filename prefix.
- `.github/workflows/test.yml` — `xcodebuild -project / -scheme` flags.

### Resolved bundle ID
`appBundleID = bundleIdPrefix.\(appName.lowercased())` resolves to
**`com.podcastr.podcastr`**. Slightly redundant; flagged here so the user can
choose to override `appBundleID` to a single-segment string later (e.g.
`com.podcastr.app`) if desired. Not changed in this pass to honour the exact
task spec.

## Stubs added

### `App/Sources/` modules
| Path | Type | Purpose |
|---|---|---|
| `Audio/AudioEngine.swift` | `final class AudioEngine` | AVPlayer wrapper, Now Playing, lock-screen, AirPlay, CarPlay |
| `Podcast/PodcastSubscription.swift` | `struct` (Codable, Sendable, Identifiable, Hashable) | Feed import / subscription model |
| `Podcast/Episode.swift` | `struct` (same) | Episode model |
| `Transcript/Transcript.swift` | `struct` (same) | Timestamped transcript model |
| `Knowledge/WikiPage.swift` | `struct` (same) | LLM-wiki page model |
| `Knowledge/VectorIndex.swift` | `final class VectorIndex` | Local RAG index (TBD storage) |
| `Voice/AudioConversationManager.swift` | `final class` | Voice STT/TTS / barge-in coordinator |
| `Briefing/BriefingComposer.swift` | `final class` | TLDR briefing generator |

All struct stubs are intentionally self-contained (no cross-references between
`Episode`, `Transcript`, `WikiPage`) so the synthesized spec can design those
relationships without rework.

### `App/Sources/Features/` view stubs
| Path | Struct | Tab? |
|---|---|---|
| `Today/TodayView.swift` | `TodayView` | yes — landing tab |
| `Library/LibraryView.swift` | `LibraryView` | yes |
| `Wiki/WikiView.swift` | `WikiView` | yes |
| `EpisodeDetail/EpisodeDetailView.swift` | `EpisodeDetailView` | no — pushed onto Library nav stack |
| `Player/PlayerView.swift` | `PlayerView` | no — reached by expanding mini-bar |
| `AgentChat/AskAgentView.swift` | `AskAgentView` | yes — "Ask" tab |
| `Voice/VoiceView.swift` | `VoiceView` | no — reached from Ask / Today |
| `Briefings/BriefingsView.swift` | `BriefingsView` | no — reached from Today |
| `Search/PodcastSearchView.swift` | `PodcastSearchView` | no — reached from Library / Wiki |

### Two deviations from the strict `<Name>View.swift` filename pattern
1. `Features/AgentChat/AskAgentView.swift` (struct `AskAgentView`). The
   folder is `AgentChat/` per task instruction, but `struct AgentChatView`
   would collide with the template's existing `Features/Agent/AgentChatView.swift`.
   The "Ask" tab uses `AskAgentView`. The `AgentChat/` folder is reserved for
   the podcast-corpus-scoped chat surface; the template's tasks-scoped chat
   stays under `Features/Agent/`.
2. `Features/Search/PodcastSearchView.swift` coexists with the template's
   `Features/Search/UniversalSearchView.swift`. The new podcast-corpus search
   uses `PodcastSearchView`; the existing `UniversalSearchView` (used by
   `HomeView`) is left untouched.

### Tab structure (extended `RootTab` in `App/Sources/App/RootView.swift`)
Tab order: **Today → Library → Wiki → Ask → Home → Settings**.
`selectedTab` defaults to `.today`.
- **Today, Library, Wiki, Ask** are the new podcast-app tabs.
- **Home** keeps the template's tasks UI live; it will be folded into the new
  surfaces (or removed) in a future PR.
- **Settings** is preserved verbatim.
- **Player** is intentionally NOT a top-level tab — it lives behind a
  persistent mini-bar (added later) that expands into `PlayerView`.
- **Voice** and **Briefings** are reached from Today / Ask, not the tab bar,
  to keep the bar focused on browsing surfaces.

## Build status

- `tuist generate` (Tuist 4.115.1) — **OK**. Generates `Podcastr.xcodeproj` /
  `Podcastr.xcworkspace`.
- `xcodebuild -workspace Podcastr.xcworkspace -scheme Podcastr
  -destination 'generic/platform=iOS Simulator' -configuration Debug
  -skipPackagePluginValidation -skipMacroValidation
  CODE_SIGNING_ALLOWED=NO CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO build`
  — **BUILD SUCCEEDED**.
- Without `-skipPackagePluginValidation`, the build fails at
  `Validate plug-in "SharedSourcesPlugin" in package "secp256k1.swift"`. CI
  must continue to pass `-skipPackagePluginValidation` (the existing
  `.github/workflows/test.yml` already does so).
- One pre-existing warning unrelated to this work:
  `AgentIdentityQRView.swift:14` — `'nonisolated(unsafe)' is unnecessary for a
  constant with 'Sendable' type 'CIContext'`. Two-line fix; left in place.
- There is a non-fatal `appintentsnltrainingprocessor` "Could not archive SSU
  artifacts" log line during build. This appears in the underlying template
  too; build still succeeds.

## What I did NOT do

- **No new SPM dependencies.** `secp256k1.swift` (P256K) is the only package,
  inherited from the template.
- **No SwiftData.** Persistence is still the inherited App Group
  `UserDefaults` JSON blob.
- **No audio playback code.** All `final class` stubs are empty `init() {}`.
- **No tab merging / template-feature deletion.** Home, Friends, Feedback,
  Agent loop, Settings all still build and run.
- **No `.gitignore` change for `.package.resolved`.** Tuist generated this
  file at `Podcastr.xcworkspace`-resolution time; it is not in `.gitignore`,
  but upstream `ios-app-template` does not track it either, so I left it
  untracked. Recommend the user decide whether to commit it for reproducible
  builds or add it to `.gitignore`.
- **No voice-notification curl.** The local notify endpoint at
  `localhost:8888/notify` returned `404 page not found` this session and
  is not load-bearing for the deliverable.

## Recommended next steps

1. **Player mini-bar.** Implement a SwiftUI overlay (probably a
   `safeAreaInset(edge: .bottom)` on the TabView) that hosts the persistent
   mini-bar and presents `PlayerView` as a `.fullScreenCover` on tap. Drive
   from `AudioEngine.state` once that exists.
2. **Lock down model relationships in the spec.** `Episode` should reference
   `PodcastSubscription.id`; `Transcript` should reference `Episode.id`;
   `WikiPage` should reference an opaque `KnowledgeScope` enum (per-podcast,
   per-episode, cross-corpus). Stubs are intentionally relationship-free
   right now to avoid pre-empting the spec.
3. **Choose a vector store** for `VectorIndex` — SQLite + cosine in Swift,
   `SVDB`, or a custom mmap'd float32 file. Affects the
   `Knowledge/VectorIndex.swift` public surface.
4. **Convert stubs to `@Observable`** as the spec settles — `AudioEngine`,
   `AudioConversationManager`, and `BriefingComposer` are likely
   `@Observable` final classes (or `actor`s for I/O-heavy work) rather than
   plain `final class`.
5. **Fold Home + Friends + Feedback into the new IA**, or move them to a
   debug-only surface before TestFlight.
6. **Decide the final bundle ID.** `com.podcastr.podcastr` is what the task
   parameters resolve to; consider overriding `appBundleID` to a tidier value
   like `com.podcastr.app` in `Project.swift`.
7. **Delete the `nonisolated(unsafe) let context = CIContext()` decoration**
   in `AgentIdentityQRView.swift`. Two-line warning cleanup.

## Files touched

The single commit on `worktree-agent-aa86a44a8641c7e16` contains:
- 3 file renames (entitlements ×2, shortcuts struct ×1)
- 26 file modifications (rename surface + RootView extension + README header)
- 16 new files (8 module stubs + 8 feature view stubs)

## Deliverable path

`/Users/pablofernandez/Work/podcast-player/.claude/research/skeleton-bootstrap-report.md`
