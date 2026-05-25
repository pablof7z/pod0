# NMP Feature Parity — Implementation Plan

**Goal:** Full feature parity with the original Swift-only podcast app, built entirely on the NMP
(Nostr Multi-Platform) architecture. All business logic in Rust; iOS and Android are thin rendering
shells. No hacks, no shortcuts.

**Reference:** Original app at `App/Sources/` (583 Swift files, ~95 K LOC). This directory is the
feature-parity specification. It ships from this tree today and must not be deleted until every
feature below passes its exit checklist.

---

## Guiding Principles

- **D0** — Rust decides, Swift/Kotlin execute. Zero business logic in Swift beyond rendering and
  capability execution.
- **D6** — every FFI boundary degrades silently (no exceptions, no crashes, only data-shaped
  errors).
- **D7** — capabilities report; they never decide.
- **D8** — reactivity ≤ 60 Hz; no polling tighter than that.
- No NIP-74. The podcast-discovery protocol is **NIP-F4** (kinds 10154 / 54 / 10064). The
  `podcast-discovery` crate must be updated to replace NIP-74.

---

## Current State (as of 2026-05-25)

### What works
- Rust kernel boots, emits a static `{"running":true,"rev":0}` snapshot.
- iOS compiles and runs: Library tab shows "No Podcasts Yet."
- `AudioCapability.swift` — full AVFoundation executor, lock-screen integration. **Ready.**
- `DownloadCapability.swift` — full background URLSession executor. **Ready.**
- `HttpCapability.swift` — synchronous GET/POST executor. **Ready.**
- `KeychainCapability.swift`, `PcstIdentityCapability.swift` — fully implemented. **Ready.**
- `PlayerActor` Rust state machine — fully implemented and tested. **Ready.**
- `DownloadQueue` Rust state machine — fully implemented and tested. **Ready.**
- `podcast-feeds` — RSS/OPML/Podcasting-2.0 parser, conditional GET. **Ready.**
- `podcast-core` — all domain types. **Ready.**
- All action wire shapes and capability command/report types. **Ready (serde-tested).**

### Critical gaps blocking everything
1. **`PodcastHandle` is empty.** Holds only `*mut NmpApp`. No state.
2. **`nmp_app_set_capability_callback` not in `NmpCore.h`.** iOS cannot receive capability
   commands from Rust.
3. **`PodcastCapabilities.shared.start()` never called** from `KernelModel`.
4. **No podcast ActionModules registered.** Every `dispatch_action` call returns
   `{"error":"unknown namespace"}`.
5. **`nmp_app_podcast_snapshot` returns a hardcoded stub** regardless of handle state.
6. **`AppStateStore` and `KernelModel` are disconnected.** `allPodcasts` is always `[]`.
7. **`PodcastUpdate` Swift struct has only `running` and `rev`.** Mismatches Rust shape.
8. **No `nmp_app_capability_report` C function.** Rust has no way to receive async reports
   from iOS capabilities (position ticks, item-end, etc.). This function must be added to
   `nmp-ffi` as part of PR 1.

---

## NMP Extensions Required

The following additions to `nmp-ffi` are required (PR 1 Rust scope):

```c
// Register the native capability router (Rust → platform synchronous calls).
// Already exists in nmp-ffi but MISSING from NmpCore.h.
void nmp_app_set_capability_callback(void *app, void *context,
    char *(*callback)(void *ctx, const char *request_json));

// Platform → kernel async capability report.
// Platform calls this when an async capability event fires (audio position
// tick, item-end, download progress, etc.).
// Returns a follow-up capability command JSON (e.g. AudioCommand::Stop after
// a sleep-timer report), or empty/null when no follow-up is needed.
// Caller must free the returned string via nmp_app_free_string.
char *nmp_app_capability_report(void *app, const char *namespace,
    const char *report_json);
```

`nmp_app_capability_report` routes through a per-namespace `CapabilityReportHandler` registered
by each podcast ActionModule. For audio: it calls `dispatch_audio_report_json(actor, report_json,
now)` on the `PlayerActor`, and returns the follow-up `AudioCommand` JSON if any.

**Why not `dispatch_action`?** The `ActionModule` contract is for user/app intents that produce
Nostr events. Capability reports are system-authored observations. Additionally, the
30-second idempotency guard on `dispatch_action` would collapse identical position ticks,
breaking playback state tracking. `nmp_app_capability_report` is the architecturally correct
path.

---

## Feature List (from `App/Sources/`)

### Tier 1 — Core Podcast Player (must ship for the app to be usable)

| # | Feature | Primary Rust component | Primary iOS capability |
|---|---------|----------------------|----------------------|
| 1 | Subscribe via RSS feed URL | `SubscribeActionModule` → `HttpCapability` → `podcast-feeds` | `HttpCapability` |
| 2 | OPML import / export | `ImportOpmlModule` → `podcast-feeds::import_opml` | `HttpCapability` |
| 3 | Library / show grid | Snapshot: `library: Vec<PodcastSummary>` | none (renders snapshot) |
| 4 | Show detail + episode list | Snapshot: `PodcastSummary.episodes` | none |
| 5 | Feed refresh (pull + background) | `RefreshActionModule` → `HttpCapability` → `podcast-feeds` | `HttpCapability` |
| 6 | Podcast search (iTunes directory) | `SearchActionModule` → `HttpCapability` | `HttpCapability` |
| 7 | Audio playback (play/pause/seek) | `PlayerActionModule` → `PlayerActor` → `AudioCapability` | `AudioCapability` |
| 8 | Variable speed (0.5× – 3.0×) | `PlayerActor.set_speed` → `AudioCommand::SetSpeed` | `AudioCapability` |
| 9 | Sleep timer | `PlayerActor.arm_sleep_timer` → `AudioReport::SleepTimerFired` | `AudioCapability` |
| 10 | Episode download (background) | `DownloadActionModule` → `DownloadQueue` → `DownloadCapability` | `DownloadCapability` |
| 11 | Auto-download policy | `PodcastStore` policy per subscription; triggered after refresh | `DownloadCapability` |
| 12 | Playback position persistence | `PlayerActor` state → `PodcastStore` (sled) | none |
| 13 | Playback queue (Up Next) | `PlaybackQueue` in `PodcastHandle`; new `podcast.queue.*` actions | none |
| 14 | Lock-screen / Control Center | `AudioCapability+NowPlaying.swift` (already wired) | `AudioCapability` |
| 15 | Chapters (Podcasting 2.0) | `podcast-feeds::parse_chapters_json`; `EpisodeSummary.chapters` | none |
| 16 | Mini player | Renders `snapshot.now_playing` | none |
| 17 | Full player view | Renders `snapshot.now_playing`; dispatches seek/speed/sleep | none |
| 18 | Downloads manager screen | Renders `snapshot.downloads` | none |
| 19 | Playback settings | `Settings` type in `podcast-core`; persisted in `PodcastStore` | none |
| 20 | New-episode notifications | `NotificationActionModule` → `UNUserNotificationCenter` | new `NotificationCapability` |

### Tier 2 — Identity & Social

| # | Feature | Primary Rust component | Notes |
|---|---------|----------------------|-------|
| 21 | Nostr keypair generation | `nostr` crate; `nmp-signer-broker` | Keychain via `PcstIdentityCapability` |
| 22 | BYOK (paste nsec) | `PcstIdentityCapability`; `pcst.identity.store_nsec` action | |
| 23 | NIP-46 remote signer | `nmp-signer-broker`; `RemoteSignerActionModule` | Uses existing NMP broker |
| 24 | Profile editing + kind:0 publish | `nostr` crate; `nmp_nip02` existing registration | `nmp_app_dispatch_action` → `nmp.nostr.publish` |
| 25 | NIP-65 relay list | Already registered by `register_defaults` | |
| 26 | NIP-F4 podcast discovery (show search) | Updated `podcast-discovery` (replace NIP-74 with 10154/54) | Relay I/O via Nostr substrate |
| 27 | NIP-F4 publish owned shows | Per-podcast keypair via `PodcastKeyStore`; `kind:10154` + `kind:10064` | |
| 28 | NIP-F4 publish episodes | `kind:54`; audio uploaded to Blossom first | `HttpCapability` for Blossom |
| 29 | Nostr episode comments (NIP-22) | `nostr` crate; relay subscription | |
| 30 | Friends / social graph | `nmp_nip02` contact list; per-friend state in `podcast-core` | |

### Tier 3 — AI Features

| # | Feature | Primary Rust component | Notes |
|---|---------|----------------------|-------|
| 31 | AI inbox triage | `InboxTriageModule` → `HttpCapability` (LLM) | OpenRouter / Ollama |
| 32 | AI agent chat (50 tools) | `AgentSessionModule`; `ConversationActor` | LLM via `HttpCapability` |
| 33 | Agent memory | `AgentMemoryModule` + `ConversationActor` | |
| 34 | Agent scheduled tasks | `AgentSchedulerModule` | |
| 35 | Transcripts (multi-source) | `TranscriptIngestModule` → `HttpCapability` (STT APIs) | `podcast-transcripts` |
| 36 | AI chapter compilation | `ChapterCompilerModule` → `HttpCapability` (LLM) | |
| 37 | Auto ad skip | Snapshot: `EpisodeSummary.ad_segments`; `PlayerActor` advances | |
| 38 | RAG / vector search | `KnowledgeStore`; embedding via `HttpCapability` | `podcast-knowledge` |
| 39 | AI wiki | `WikiModule` → `HttpCapability` (LLM) + RAG | |
| 40 | AutoSnip / clip composer | `ClipModule`; boundary refinement via LLM | |
| 41 | AI briefings | `BriefingScheduler` + composer → `HttpCapability` | `podcast-briefings` |
| 42 | Voice mode (STT + TTS + barge-in) | `VoiceActionModule`; ElevenLabs via `HttpCapability` | `AudioCapability` for recording |
| 43 | Agent-generated podcasts (TTS) | `TtsEpisodeModule` → `HttpCapability` (ElevenLabs) | |
| 44 | Nostr agent-to-agent (NIP-17) | Already registered by `register_defaults` | |
| 45 | Agent categorization | `CategorizationModule` → `HttpCapability` (LLM) | |
| 46 | AI agent picks (Home featured) | `AgentPicksModule` → `HttpCapability` (LLM) | |

### Tier 4 — Platform Integration

| # | Feature | Notes |
|---|---------|-------|
| 47 | CarPlay | `CarPlayCapability` + `PlatformCapability` wiring |
| 48 | Widgets / Live Activity | `PlatformCapability.writeWidgetSnapshot` (partially done) |
| 49 | AppIntents / Siri | `SiriActionModule`; `podcast.siri.play_latest` etc. |
| 50 | Spotlight indexing | `SpotlightCapability` |
| 51 | Handoff | `PlatformCapability.donateHandoff` (partially done) |
| 52 | iCloud settings sync | `SettingsSyncCapability` |
| 53 | Local notifications | `NotificationCapability` |
| 54 | Android second-platform | All Tier 1–3 features via the same Rust kernel; `AudioCapabilityStub` → ExoPlayer |

---

## PR Sequence

### PR 1 — Core Infrastructure (blocks everything else)

**Rust (`apps/nmp-app-podcast/`):**
- `Cargo.toml`: add `sled = "0.34"` (embedded KV store, zero system deps)
- `src/store.rs` (new): `PodcastStore` — sled-backed, holds `Podcast` + `Vec<Episode>` keyed by
  feed URL; `subscribe`, `unsubscribe`, `all_podcasts`, `episodes_for`, `update_position` methods
- `src/ffi/handle.rs`: extend `PodcastHandle` with:
  - `player_actor: Arc<Mutex<PlayerActor>>`
  - `download_queue: Arc<Mutex<DownloadQueue>>`
  - `store: Arc<Mutex<PodcastStore>>`
  - `rev: Arc<AtomicU64>`
- `src/ffi/snapshot.rs`: wire `build_snapshot_payload` to read from handle — populate `now_playing`,
  `downloads`, `library` / `podcasts`, `widget`, bump `rev` atomically
- `src/ffi/snapshot.rs`: add `PodcastUpdate` fields:
  `library: Vec<PodcastSummary>`, `podcasts: Vec<PodcastSummary>`,
  `active_account: Option<AccountSummary>`, `toast: Option<String>` (already present),
  `schedule_label: Option<String>` on `BriefingSnapshot`
- `src/ffi/actions/subscribe.rs` (new): `podcast.subscribe` ActionModule — decodes
  `SubscribeAction { feed_url }`, builds `HttpRequest` via `podcast-feeds::build_feed_request`,
  dispatches via `nmp_app_dispatch_capability`, parses `HttpResult` with
  `podcast-feeds::handle_feed_response`, stores in `PodcastStore`
- `src/ffi/actions/player.rs` (new): `podcast.player.*` ActionModule — routes play/pause/seek/
  set_speed/set_sleep_timer/stop to `PlayerActor`, emits `AudioCommand` via capability socket
- `src/ffi/actions/download.rs` (new): `podcast.player.download` etc. ActionModule — routes to
  `DownloadQueue`, emits `DownloadCommand` via capability socket
- `src/ffi/register.rs`: register all three ActionModules after `register_defaults`
- **`nmp-ffi` extension**: add `nmp_app_capability_report(app, namespace, report_json) -> *mut c_char`
  — platform→kernel async report path (required for position ticks, item-end events)

**iOS (`ios/Podcast/`):**
- `Bridge/NmpCore.h`: add `nmp_app_set_capability_callback`, `nmp_app_dispatch_capability`,
  `nmp_app_capability_report` declarations
- `Bridge/KernelBridge.swift` `init`: call `nmp_app_set_capability_callback` with a C trampoline
  that routes to `PodcastCapabilities.shared.handleJSON(_:)`
- `App/KernelModel.swift` `start()`: call `PodcastCapabilities.shared.start()` before
  `kernel.start(...)`
- `Capabilities/AudioCapability.swift`: wire `attach(sendReport:)` so all async reports call
  `nmp_app_capability_report(raw, "nmp.audio.capability", report_json)` and process any returned
  follow-up `AudioCommand`
- `Capabilities/DownloadCapability.swift`: same — wire report closure to
  `nmp_app_capability_report(raw, "nmp.download.capability", report_json)`
- `Bridge/Generated/PodcastTypes.generated.swift`: replace the stub `PodcastUpdate` with the full
  shape matching Rust: `library`, `podcasts`, `nowPlaying`, `downloads`, `activeAccount`, `toast`,
  `agent`, `voice`, `briefing`, `widget` (all optional, `@SnakeCase` decoding)
- Delete `ios/Podcast/Podcast/Compat/AppStateStore` and all compat stubs that are now replaced.
  Views read `@Environment(KernelModel.self)` directly; `model.snapshot?.subscriptions` replaces
  `store.allPodcasts`
- `Compat/ServiceStubs.swift`: replace `SubscriptionService.addSubscription` →
  `model.dispatch("podcast.subscribe", ["feed_url": url])`
- Replace `PlaybackState.play/pause/seek/setSpeed/setSleepTimer` → corresponding
  `model.dispatch("podcast.player.*", [...])` calls
- Wire `MiniPlayerView` to `model.snapshot?.nowPlaying`

**After PR 1:** User can subscribe to a podcast (RSS fetched by iOS HttpCapability, parsed by Rust
`podcast-feeds`, stored in sled), see it in the Library grid, tap an episode and hear it play
through AVFoundation, see the mini-player with correct metadata, and download episodes. Position
ticks flow Rust→iOS→Rust correctly.

---

### PR 2 — Library UX (feed refresh, show detail, OPML, search)

- Rust: `podcast.refresh` and `podcast.refresh_all` ActionModules
- Rust: `podcast.search_itunes` ActionModule → `HttpCapability` → iTunes Search API JSON
- Rust: `podcast.import_opml` / `podcast.export_opml` ActionModules using `podcast-feeds`
- iOS: `ShowDetailView` reads `model.snapshot?.subscriptions` for episode list
- iOS: Pull-to-refresh dispatches `podcast.refresh`; `lifecycleForeground` dispatches
  `podcast.refresh_all`
- iOS: `AddShowSheet` search tab dispatches `podcast.search_itunes`; results surfaced via
  snapshot field `search_results: Vec<PodcastSummary>` (new snapshot field)

---

### PR 3 — Full Player Experience

- Rust: Chapters wired into `EpisodeSummary.chapters` from `PodcastStore`
- Rust: `PlaybackQueue` in `PodcastHandle`; `podcast.queue.*` actions
- Rust: `SetMetadata` AudioCommand variant for lock-screen artwork/title
- Rust: `podcast.player.skip_forward` / `podcast.player.skip_backward` actions
- iOS: Full player view wired to `model.snapshot?.nowPlaying`
- iOS: Sleep timer chip, speed chip, route picker — all dispatch through `model.dispatch`
- iOS: Queue (Up Next) sheet renders from snapshot queue field
- iOS: `AudioCapability+RemoteCommands.swift`: add skip-fwd/bwd remote commands

---

### PR 4 — Identity (NMP-native)

- Rust: `pcst.identity.*` ActionModules using `nmp-signer-broker` + `nostr` crate + `PcstIdentityCapability`
- Rust: `generateKey()` → Rust generates keypair, stores nsec in Keychain via capability, snapshot
  emits `activeAccount: AccountSummary`
- Rust: `importNsec()` → stores via `PcstIdentityCapability`
- Rust: NIP-46 remote signer → `nmp-signer-broker` handles entirely (no new Swift code)
- Rust: Profile publish → `nmp_nip02` (already registered) handles `kind:0`
- iOS: `IdentityRootView` renders `model.snapshot?.activeAccount`; no UserIdentityStore

---

### PR 5 — Downloads Complete + Auto-Download

- Rust: `PodcastStore` stores `local_path` per episode after download completes
- Rust: Auto-download policy stored per subscription; triggered after feed refresh by
  `RefreshActionModule`
- Rust: `podcast.player.download` and `podcast.player.delete_download` actions
- iOS: Episode rows show download state from `model.snapshot?.downloads`
- iOS: Downloads manager renders from snapshot
- iOS: `DownloadCapability`: offline-first — checks local file before streaming

---

### PR 6 — NIP-F4 Podcast Discovery (replaces NIP-74)

- Update `podcast-discovery` crate:
  - `KIND_SHOW` → 10154 (was 30074)
  - `KIND_EPISODE` → 54 (was 30075)
  - Remove `d_tag` from show; `coordinate()` → `"10154:<pubkey-hex>"`
  - Episodes identified by event ID (no `d_tag`); no `["a", ...]` tag
  - `["audio", url, mime]` replaces `imeta` block
  - `["description", ...]` replaces `["summary", ...]`
  - New `AuthorClaim` build helper for `kind:10064`
- Add `PodcastKeyStore` to `podcast-discovery` or `podcast-core`: per-podcast keypair management
  backed by `PcstIdentityCapability` (Keychain slot `"podcast-privkey-<uuid>"`)
- Rust: `podcast.discover_nostr` ActionModule → relay subscription for `kind:10154`
- Rust: `podcast.publish_show` / `podcast.publish_episode` ActionModules → sign with podcast
  keypair → publish via `nostr` crate
- iOS: `NostrDiscoverForm` dispatches `podcast.discover_nostr`; results in snapshot

---

### PRs 7–N — AI + Platform (parallel agents)

These can be developed in parallel once PR 1–3 are merged:

- **M-Transcripts** (PR 7): `TranscriptIngestModule` → `HttpCapability` (STT APIs) →
  `podcast-transcripts` parse/chunk → `KnowledgeStore`
- **M-Agent** (PR 8): `AgentSessionModule` + `ConversationActor` + 50 tools via `HttpCapability`
- **M-Briefings** (PR 9): `BriefingScheduler` wired; `BriefingComposer` → `HttpCapability` (LLM +
  ElevenLabs TTS)
- **M-Voice** (PR 10): `VoiceActionModule`; ElevenLabs TTS + Apple STT via `HttpCapability`
- **M-Platform** (PR 11): CarPlay, Widgets, AppIntents, Spotlight, Handoff — all via
  `PlatformCapability` already partially stubbed
- **M-Android** (PR 12): ExoPlayer wired to `AudioCapabilityStub`; `nmpCapabilityReport` wired to
  `nmp_app_capability_report`

---

## What Must NOT Be Done

- No reimplementing NIP-46 in Swift. Use `nmp-signer-broker` entirely.
- No reimplementing Nostr event signing in Swift. The `nostr` crate owns all signing.
- No NIP-74. Every reference to kinds 30074/30075 must be replaced with 10154/54/10064.
- No `AppStateStore` or compat stubs remaining after their PR. Delete on contact.
- No `App/Sources/` code pulled into the new target. Feature parity means the same user outcomes
  via a completely different implementation stack.
- No SQL unless the feature genuinely needs relational queries. Start with `sled` (key-value);
  migrate to `rusqlite` if complexity demands it.

---

## Exit Criteria for "Feature Parity"

The stop hook condition is met when:

1. All 74 features listed in `App/Sources/` (catalogued by audit agent 2026-05-25) work through
   the NMP stack.
2. `App/Sources/` can be deleted without breaking any user flow.
3. iOS and Android ship from the same Rust kernel build.
4. No compat stub files remain in `ios/Podcast/Podcast/Compat/`.
5. `cargo test --workspace` passes and `xcodebuild test` passes.
6. `docs/plan.md` milestone table shows all milestones ✅.
