# Template Architecture and Extension Plan

A deep read of every Swift file under `App/Sources/` plus the build, CI, and entitlement scaffolding, distilled into the architecture we inherit and the surgical work we will do to turn it into a podcast player with a knowledge-grounded embedded agent.

The task brief was written from the original template README. The actual codebase has evolved well past the README. This report reconciles the two and recommends a concrete extension shape. The most important corrections up front:

- **ElevenLabs is already fully integrated** (Keychain store, voices service, TTS preview, settings UI). We extend, not add.
- **Nostr is a complete subsystem** (relay service, key pair, Bech32, allowlist/blocklist, pending approvals, agent reply bridge). Keep verbatim.
- **The agent loop file is `Features/Agent/AgentChatSession.swift`**, not `Agent/AgentSession.swift` as the README and task claim. There is also `AgentRelayBridge.swift` that runs the same loop for Nostr-inbound messages.
- **Tool dispatchers are already split** into `AgentTools+Items.swift`, `+DueDates`, `+NotesMemory`, `+Reminders`, `+Search`, with the schema in `AgentToolSchema.swift`. New podcast tools should live alongside these as `AgentTools+Podcast.swift`, `+Wiki.swift`, `+Briefing.swift` — not in a new `AgentExtensions/` folder.
- **iOS 26 deployment target, Swift 6 strict concurrency, and Liquid Glass `.glassEffect()`** are already configured in `Project.swift` and `GlassSurface.swift`. No work needed there.

## 1. What we have — module map and runtime model

`App/Sources/` is ~25,500 lines of Swift across ~150 files in eight top-level groups. Largest file is `ItemDetailSheet.swift` at 494 lines (under the 500 hard limit).

- **Entry** — `AppMain.swift`, `App/RootView.swift`, `App/AppDelegate.swift`. SwiftUI `@main`, `TabView`, shake handler, deep-link routing, notification action buttons.
- **Domain** — `Item`, `Note`, `Friend`, `AgentMemory`, `Anchor`, `Settings`, `AgentActivity`, `NostrPendingApproval`. All `Codable + Sendable`; every decoder uses `decodeIfPresent` for forward-compat.
- **State** — `State/AppStateStore.swift` plus six extension files (Items, Notes, Memories, Friends, Nostr, AgentActivity, DerivedViews). `@MainActor @Observable`. Single source of truth.
- **Persistence** — `State/Persistence.swift` encodes the entire `AppState` to JSON, writes to App Group `UserDefaults` keyed `apptemplate.state.v1`.
- **Agent** — `Agent/AgentTools.swift` plus `+Items`, `+NotesMemory`, `+Reminders`, `+DueDates`, `+Search`, with `AgentToolSchema.swift` and `AgentPrompt.swift`. The streaming loop lives in `Features/Agent/AgentChatSession.swift` and `AgentOpenRouterClient.swift`. `AgentRelayBridge.swift` runs the same loop for Nostr-inbound DMs.
- **Services** — `KeychainStore`, `OpenRouterCredentialStore`, `ElevenLabsCredentialStore`, `NostrCredentialStore`, `BYOKConnectService` (PKCE), `NostrRelayService` (WebSocket + kind-1 + reconnect), `NostrKeyPair` (P256K), `Bech32`, `NotificationService`, `BadgeManager`, `SpotlightIndexer`, `iCloudSettingsSync`, `DataExport`, `DeepLinkHandler`, `VoiceItemService` (`SFSpeechRecognizer` dictation), `ChatHistoryStore`, `ReviewPrompt`, `UserIdentityStore`.
- **Design** — `AppTheme` (split by concern), `GlassSurface` (calls native iOS 26 `.glassEffect()`), `Haptics`, `PressableStyle`, `ShakeDetector`, `MarkdownView`, `AsyncButton`.
- **Features** — `Home`, `Agent`, `Feedback`, `Friends`, `Onboarding`, `Search`, `Settings`. No feature writes state outside the store.
- **Intents + Widget** — App Intents for Siri/Shortcuts; widget extension reads the App Group `UserDefaults` blob via `WidgetPersistence`.

Mutation fan-out: `state.didSet` triggers `Persistence.save` (whole-blob JSON), `SpotlightIndexer.reindex`, `BadgeManager.sync`, `WidgetCenter.shared.reloadAllTimelines()`, and `iCloudSettingsSync.shared.push`. Cheap at hundreds of items, ruinous at thousands of transcript chunks — see Section 6.

Agent loop (`AgentChatSession.runAgentTurns`): refresh system prompt → call `AgentOpenRouterClient.streamCompletion` → accumulate SSE delta chunks into `(assistantMessage, toolCalls)` → dispatch each tool via `AgentTools.dispatch` → append a `role: tool` JSON-result message → repeat up to `maxTurns = 20`. Cancellation, error, retry already handled. Reused at `maxTurns = 8` for Nostr DM-driven actions in `AgentRelayBridge`.

Friends: `Friend.identifier` is a Nostr hex pubkey; `addFriend` auto-inserts into `nostrAllowedPubkeys`. Items created by a friend's agent carry `requestedByFriendID` + `requestedByDisplayName` for provenance.

Feedback: shake → `FeedbackWorkflow` state machine (idle → composing → awaitingScreenshot → annotating); persisted threads in `FeedbackStore` (`Documents/feedback_threads.json`, separate from `AppState`).

## 2. Keep verbatim

Anything that already does its job is not in scope to change.

- `KeychainStore`, `OpenRouterCredentialStore`, `ElevenLabsCredentialStore`, `NostrCredentialStore`, `BYOKConnectService` (PKCE flow with state validation).
- `NostrRelayService`, `NostrKeyPair`, `Bech32`, `NostrPendingApproval`, the entire `nostrAllowedPubkeys` / `nostrBlockedPubkeys` / `nostrPendingApprovals` ACL, `AgentRelayBridge`.
- `Friend` model and all `AppStateStore+Friends` operations.
- Feedback subsystem end to end: `ShakeDetector`, `FeedbackWorkflow`, `FeedbackView`, `FeedbackStore`, `ScreenshotAnnotationView`, `FeedbackBubble`, `FeedbackThreadDetailView`, `FeedbackThreadRow`. Wire `FeedbackView.performSubmission` to whatever backend we choose later; that hook already exists.
- `Haptics`, `PressableStyle`, `GlassSurface` (already calls native iOS 26 `.glassEffect()`), `ShakeDetector`, `AppTheme.{Spacing,Corner,Layout}`.
- `BadgeManager`, `iCloudSettingsSync`, `SpotlightIndexer` (we will extend its catalog, not its mechanism), `NotificationService` (action buttons + categories).
- `Project.swift` skeleton, the entitlements file, the Info.plist scaffolding, `ci_scripts/*`, both GitHub workflows. Update env vars and the App Group ID; do not touch the structure.
- `ChatHistoryStore` for chat-only history (kept on disk in `Documents`, capped at 100 messages, atomic writes).
- The `Anchor` discriminated union — it is the right abstraction for linking notes/memories to new domain types (`.episode(id:)`, `.podcast(id:)`, `.briefing(id:)` cases will be added).
- The agent activity log + per-batch undo. New podcast tools must record `AgentActivityEntry` exactly the same way; this is the user's safety net.

## 3. Extend surgically

Existing components that grow new capability without changing shape.

- **`AppStateStore` / `AppState`.** Add `subscriptions`, `nowPlaying: NowPlayingSnapshot?`, `briefingScripts`, and new `HomeAction` cases `.openEpisode(UUID)`, `.openPlayer`, `.openBriefing(UUID)`. Items/notes/memories stay in `AppState`. Episodes, transcripts, wiki pages do **not** — see Section 6.
- **`AgentTools` + `AgentToolSchema`.** Add tool names to `AgentTools.Names`, schema entries to `schema`, dispatch routing to new files: `AgentTools+Podcast.swift` (`play_episode_at`, `pause_playback`, `set_now_playing`, `find_similar_episodes`, `summarize_episode`, `search_episodes`, `open_screen`), `+Wiki.swift` (`query_wiki`), `+RAG.swift` (`query_transcripts`), `+Briefing.swift` (`generate_briefing`), `+Web.swift` (`perplexity_search`). Every mutating tool records an `AgentActivityEntry` with a new `AgentActivityKind` case so undo keeps working.
- **`AgentPrompt`. The load-bearing change.** Today it dumps every active item, recent note, memory, and friend into the prompt. That fails at 50 podcasts × 200 episodes. Rewrite to *inventory + handles*: subscription list with episode counts, last-listened bookmark, a few "new this week" episodes, current `nowPlaying` position, tool descriptions, and an explicit instruction to call `search_episodes` / `query_transcripts` / `query_wiki` to read further. The agent's eyes become its tools; its memory is a vector store.
- **`Settings`.** Add embedding provider source + Keychain key (only if OpenRouter cannot serve embeddings — Section 8), Perplexity key metadata, transcription preference (`publisher | scribe | local`), briefing voice (reuse `elevenLabsVoiceID`), download-over-cellular toggle, default playback rate, skip-back / skip-forward seconds.
- **`AppTheme`** — tokens for the editorial design system from UX 15: serif/display type ramp, hero palette, player-chrome motion curves, scrubber colorways.
- **`Anchor` enum** — add `.episode`, `.podcast`, `.briefing`, `.transcriptChunk` cases. The discriminated-union pattern is exactly right.
- **`SpotlightIndexer`** — index episodes and briefings.
- **`DeepLinkHandler` + `RootView`** — new routes `apptemplate://episode/<id>`, `…://play/<id>?t=…`, `…://briefing/<id>`.
- **Widget** — add `NowPlayingWidget` (lock-screen playback control) and `BriefingWidget` (one-tap today's briefing). Same App-Group `UserDefaults` snapshot mechanism.

## 4. Net-new modules

Non-feature engine layers:

- **`Audio/`** — `AudioSessionCoordinator` (singleton; owns all `AVAudioSession` transitions), `PlaybackEngine` (AVPlayer wrapper), `NowPlayingMetadataPublisher` (`MPNowPlayingInfoCenter`), `RemoteCommandHandler` (`MPRemoteCommandCenter`), `AudioRouteObserver`.
- **`Podcast/`** — `Subscription`, `Episode`, `RSSFeedParser`, `OPMLImporter`, `EnclosureDownloader`, `FeedRefreshScheduler`.
- **`Transcript/`** — `TranscriptChunk` (`episodeID`, `startSec`, `endSec`, `speaker`, `text`), `TranscriptSource` (`publisher | scribe | local`), `PublisherTranscriptFetcher`, `ScribeTranscriptionClient` (ElevenLabs Scribe), `TranscriptChunker`.
- **`Knowledge/`** — `WikiPage`, `WikiGenerator` (LLM-driven, in the spirit of nvk/llm-wiki), `EmbeddingService`, `VectorStore` protocol with a `SQLiteVecStore` impl, `RAGQueryService`.
- **`Voice/`** — `AudioConversationManager` state machine (idle → listening → thinking → speaking → bargeIn → listening), `BargeInDetector`, `TTSStreamer` (ElevenLabs streaming).
- **`Briefing/`** — `BriefingComposer` (script with `<beat>` markers + episode anchors), `BriefingScript`, `BriefingPlayer` (chains TTS clips; interrupt + resume to nearest `<beat>`).

New feature folders: `Features/{Player, Library, Episode, Wiki, Voice, Briefings, Today}`. Existing `Features/{Agent, Friends, Feedback, Settings, Onboarding, Search}` continue to live where they are; `Search` extends with semantic results.

The task brief proposed `App/Sources/AgentExtensions/`. Don't. Every tool dispatcher already lives under `Agent/`; splitting the agent across two folders fragments a coherent module. New tool files belong in `Agent/`.

## 5. Concurrency model

`AppStateStore`, `AgentChatSession`, `AgentRelayBridge`, `VoiceItemService`, `NostrRelayService`, and `ChatHistoryStore` are all `@MainActor`. That stays. New components follow the same rule:

- **Main-actor**: anything that writes to `AppStateStore` or owns SwiftUI-observable state (`AudioConversationManager`, `BriefingPlayer` state, `RAGQueryService` request-coordinator, `PlaybackEngine`'s observable wrapper).
- **Background**: pure CPU/IO work — RSS parsing, OPML parse, transcript chunking, embedding HTTP calls, vector-store reads — runs on dedicated `Task.detached` or background actors that return `Sendable` value types and hop back to `@MainActor` for the write.
- **System frameworks**: `AVPlayer` callbacks fire on a private queue; the `PlaybackEngine` translates them through a `MainActor.run { … }` boundary before touching state.
- **`SFSpeechRecognizer` callbacks** already use `MainActor.assumeIsolated` in the existing `VoiceItemService` — pattern carries over.
- **Background tasks** (`BGTaskScheduler`): `BGAppRefreshTask` for RSS poll (≤30 s), `BGProcessingTask` for transcription + embedding indexing (longer, deferrable, can require power). Identifiers registered in `Info.plist`.

Swift 6 strict concurrency is on; we keep it on. Inter-actor boundaries pass `Sendable` value types only.

## 6. Persistence strategy and migration

The current `Persistence.save` rewrites the entire `AppState` JSON to App-Group `UserDefaults` on every mutation. That is fine for items/notes; it would be catastrophic for thousands of transcript chunks each ~250 tokens, plus wiki pages, plus embeddings. Keeping the whole-blob model would also force every Spotlight reindex and widget reload to scan the whole graph. Recommendation:

- **SwiftData `ModelContainer`** stored in the App Group container (so the widget can read it if needed) for: `Subscription`, `Episode`, `EpisodeDownload`, `TranscriptChunk`, `WikiPage`, `BriefingScript`, `EmbeddingRef` (vector ID + back-reference). Schema versions explicit; migrations handled by `SchemaMigrationPlan`.
- **Vector store** as a separate sqlite-vec file in the App Group container (or use `SVDB`/`USearch` if SPM dependencies are acceptable). Rows: `(id, source: "transcript"|"wiki", chunkID, vector)`. Reads happen off-main; writes batched.
- **`AppState` + UserDefaults** continues to hold what it already holds plus settings, ACLs, friends, agent memories, agent activity log (already capped at 200), pending approvals, briefing index. Items/notes do not migrate — they remain in `AppState` for backward compat.
- **Widget compatibility**: keep a small `NowPlayingSnapshot` struct in App-Group `UserDefaults` (episode title, artwork URL, current second, isPlaying). The widget reads only that. We do **not** give the widget a SwiftData container — the existing widget already reads from the `UserDefaults` blob via `WidgetPersistence`, and that pattern is the right one.

Migration plan: ship empty SwiftData schema in v1.1; first launch creates the container; existing `AppState` is untouched. No data movement is required because the new entities have no v1 predecessors.

## 7. App lifecycle and background

Required additions to `App/Resources/Info.plist`:

- `UIBackgroundModes`: `audio`, `fetch`, `processing`.
- `BGTaskSchedulerPermittedIdentifiers`: feed-refresh ID, transcription ID, embedding-index ID.
- `NSAppleMusicUsageDescription` (lock-screen now-playing).
- `MPNowPlayingInfoCenter` and `MPRemoteCommandCenter` are framework APIs, no plist entry needed.

`AppTemplate.entitlements` additions: nothing required for background audio. CarPlay needs `com.apple.developer.carplay-audio` (phase-2). Push notifications optional.

`AppMain` registers `BGTaskScheduler` handlers in `application(_:didFinishLaunchingWithOptions:)`. `FeedRefreshScheduler` schedules the next refresh on each foreground event. Long-running transcription and embedding go through `BGProcessingTask` so the system can pick a moment with power and Wi-Fi.

## 8. Settings and secrets

Existing Keychain entries: `OpenRouterCredentialStore`, `ElevenLabsCredentialStore`, `NostrCredentialStore`. Pattern is uniform: a service-scoped `(service, account)` pair, `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, all access through a typed enum.

New entries follow the same pattern:

- `PerplexityCredentialStore` — for `perplexity_search`.
- `EmbeddingProviderCredentialStore` — only required if OpenRouter does not serve the embedding model we choose. Worth verifying explicitly before we wire `EmbeddingService` to OpenRouter; OpenRouter's embeddings coverage has historically been thinner than its chat coverage. If we end up calling Voyage / Cohere / OpenAI directly, this store handles their key.

Settings metadata (non-secret) extends `Settings` with: `transcriptionPreference`, `embeddingProvider`, `embeddingModel`, `briefingVoiceID`, `briefingDefaultLengthMin`, `downloadOverCellular`, `defaultPlaybackRate`, `skipBackSeconds`, `skipForwardSeconds`. `iCloudSettingsSync` already merges arbitrary `Settings` fields key-by-key, so syncing across devices comes for free.

## 9. File-size discipline

AGENTS.md sets soft 300 / hard 500. Current ceiling is `ItemDetailSheet.swift` at 494. Follow the existing extension-per-concern pattern: `PlaybackEngine.swift` + `PlaybackEngine+RemoteCommands.swift` + `PlaybackEngine+NowPlaying.swift`; `AudioConversationManager.swift` + `+BargeIn.swift` + `+TTSPlayback.swift`. Feature views split into row-component files exactly like `Home/` is structured today.

## 10. Build and CI

`Project.swift`: rename `appName` and `bundleIdPrefix` (currently `AppTemplate` / `com.pablofernandez`) to the podcast player identity. Add SPM dependencies — candidates: `FeedKit` (RSS/OPML), `SVDB` or `USearch` for the vector store, optionally `swift-soup` for HTML show-notes. Cap at three new deps. URL scheme already wired; add routes in code. `.github/workflows/testflight.yml` env vars updated; `test.yml` already runs `xcodebuild test`. CI scripts unchanged.

## 11. Testing strategy

Existing `AppTests.swift` (~213 lines) covers store mutations, Codable round-trips, prompt construction, data export. Continue the pattern; consider Swift Testing (`@Test`) for net-new files (iOS 26 / Swift 6 supports both side by side).

Unit-testable directly: RSS/OPML parsers (fixture-driven), `TranscriptChunker`, `AgentPrompt` (snapshot tests for the new inventory branches), `AgentTools.dispatch` (table-driven: args → JSON result + store delta), `BriefingComposer` (fake LLM, verify beat anchors round-trip), `AudioSessionCoordinator` (protocol stub for `AVAudioSession`).

Integration: a fake `AgentOpenRouterClient` driving the loop end-to-end with scripted SSE — exercises tool-calling, retry, cancellation, multi-turn. UI: snapshot tests for the editorial design system across dark/light + dynamic type.

## 12. Risks and big questions

1. **Agent context strategy is the load-bearing decision.** Today's `AgentPrompt.build` dumps everything; that breaks at thousands of chunks. Commit to *inventory + handles + RAG-via-tools* before writing any new tools, or we end up with two competing strategies in the same prompt.
2. **AVAudioSession is one device, three callers.** `VoiceItemService` uses `.record`, player wants `.playback`, conversation wants `.playAndRecord + .voiceChat`. Single `AudioSessionCoordinator` owns transitions or routing breaks.
3. **OpenRouter embeddings coverage** — verify before designing around it. Separate provider means another Keychain entry + BYOK flow.
4. **Migration sequencing.** Don't ship SwiftData and the agent-prompt rewrite in the same release. Land SwiftData empty first; add entities feature-by-feature.
5. **Transcript-ingestion latency vs UX promise.** "Talk to all your podcasts" implies the transcript is ready. Scribe isn't instant. Need a "transcript pending" UX plus a background prefetch queue from the moment the user subscribes.
6. **Briefing interruption integrity.** Duck/stop TTS within 200 ms, hold script position, resume at the next `<beat>`. State-machine spec, not just implementation.
7. **CarPlay** — its own scene + entitlement; phase-2.
8. **Nostr-mediated agent commands.** The relay bridge already exists. An extended toolset means a friend's DM could `play_episode_at` on the user's device. Audit which tools are safe to expose; gate the rest behind explicit approval.

## Final structure (annotated)

```
App/Sources/
├── AppMain.swift                            // keep
├── App/{RootView,AppDelegate}.swift         // extend: tabs, deep-links, BGTask register
├── Domain/                                  // keep all; extend AppState, Settings, Anchor
├── State/AppStateStore.swift (+exts)        // extend: +Subscriptions, +NowPlaying, +Briefings
├── Persistence/                             // new folder
│   ├── Persistence.swift                    // keep — AppState/UserDefaults blob
│   ├── SwiftDataContainer.swift             // new — ModelContainer in App Group
│   └── VectorStore.swift                    // new — sqlite-vec wrapper
├── Services/                                // keep; add Perplexity (and maybe Embedding) Keychain store
├── Design/                                  // keep; AppTheme grows editorial tokens
├── Audio/                                   // new — AudioSessionCoordinator, PlaybackEngine, NowPlaying
├── Podcast/                                 // new — Subscription, Episode, RSS, OPML
├── Transcript/                              // new — TranscriptChunk, Scribe client, chunker
├── Knowledge/                               // new — Wiki, EmbeddingService, RAGQueryService
├── Voice/                                   // new — AudioConversationManager, BargeInDetector, TTSStreamer
├── Briefing/                                // new — BriefingComposer, BriefingPlayer
├── Agent/
│   ├── AgentTools.swift / AgentToolSchema.swift / AgentPrompt.swift  // extend (rewrite Prompt)
│   ├── AgentTools+{Items,NotesMemory,Reminders,DueDates,Search}.swift // keep
│   ├── AgentTools+{Podcast,RAG,Wiki,Briefing,Web}.swift              // new
│   └── AgentRelayBridge.swift               // keep; audit exposed tools
├── Features/
│   ├── Home/, Friends/, Feedback/, Onboarding/, Settings/, Agent/   // keep
│   ├── Search/                              // extend — semantic + episode-scoped
│   └── Today/, Library/, Episode/, Player/, Wiki/, Voice/, Briefings/ // all new
└── Intents/                                 // extend: PlayEpisodeIntent, OpenBriefingIntent
App/Widget/Sources/
├── ItemsWidget.swift                        // retire after spec
├── NowPlayingWidget.swift                   // new
└── BriefingWidget.swift                     // new
App/Resources/
├── Info.plist                               // extend: UIBackgroundModes, BGTask IDs
└── AppTemplate.entitlements                 // keep (CarPlay added phase-2)
Project.swift, Tuist.swift                   // update bundle ID, app name; add SPM deps
ci_scripts/, .github/workflows/              // keep; update env
```

File path: `/Users/pablofernandez/Work/podcast-player/.claude/research/template-architecture-and-extension-plan.md`
