# NMP Feature Parity - Execution Status

**Goal:** full feature parity with the original Swift-only podcast app, built
on the NMP architecture. Business logic belongs in Rust; iOS and Android are
thin rendering/capability shells.

**Reference:** `App/Sources/` remains the feature-parity specification. Do not
delete it until all exit criteria at the bottom of this file pass.

## Status Labels

| Label | Meaning |
|---|---|
| Done | User-visible behavior works through the NMP stack and has focused validation. |
| Partial | Core path exists, but important behavior, persistence, platform parity, or validation is missing. |
| Scaffold | Types, UI, or action shells exist, but real domain/provider/relay/LLM logic is absent. |
| Wrong | Current code contradicts the architecture or protocol contract and must be corrected before layering on it. |
| Blocked | Work depends on another listed item. |

## Current Snapshot - 2026-05-26

The large PR stack has merged, but it does not equal feature parity. GitHub
reported zero open PRs after PR #105. Many branches created screens,
projections, action namespaces, and local heuristics; several still need real
logic, relay/provider integration, or removal of compat shims.

Recent corrective PRs changed the status, but not the final exit criteria:
PR #89 fixed the current NIP-F4 wire builders/parsers, PR #93 replaced fake
pubkey derivation with real secp256k1 derivation, PR #95 landed local Ollama
provider support, and PR #96 fixed restored-playback auto-download behavior.
PR #101 finished the Rust download queue projection/report loop, and PR #102
cleared the known iOS focused-test compile blockers. PR #103 briefly restored
Play Latest, then PR #104 reconciled the AppIntents state by hiding Play
Latest until it can route through Rust-owned `podcast.siri.play_latest`; PR
#105 aligned the remaining Pod0 brand test assertions. The remaining NIP-F4
work is persistence, signing, relay publication, relay-backed discovery,
author claims, and deletion cleanup.

The stale PR-1 status from the original plan is no longer true: `PodcastHandle`
has state, snapshot fields exist, actions are registered, and iOS generated
types are broader than the original stub. The remaining problem is quality of
completion, not absence of all infrastructure.

## Doctrine Checks

- **D0 Rust decides:** still violated anywhere iOS chooses business policy
  because the Rust action or snapshot is missing. Known pressure areas:
  identity/profile local storage, provider credential settings, some platform
  integration, and scaffold AI surfaces.
- **D6 errors as data:** mostly followed at FFI boundaries; keep this as a
  validation point for every new capability/report path.
- **D7 capabilities report; they never decide:** watch capability code for
  policy drift, especially iCloud sync, Spotlight, CarPlay, notifications,
  Live Activity, voice, and HTTP provider integration.
- **D8 reactivity <= 60 Hz:** current polling/content-hash improvements reduce
  churn, but full push delivery through the NMP update sink remains open.
- **NIP-F4 is canonical:** PR #89 corrected the active NIP-F4 builders/parsers to
  the canonical wire shape. Relay publish/discovery, author claims, and terminology
  still need validation before this doctrine check can be marked satisfied.

## Core PR Sequence Status

| Slice | Status | What Still Needs To Happen |
|---|---|---|
| PR 1 - Core infrastructure | Partial | Keep JSON persistence if chosen, but document it as the canonical store or migrate to the planned storage. Finish push-style snapshot delivery through the NMP update sink. Unify capability routing so there is one router and capabilities own their threading. Delete replaced compat shims. |
| PR 2 - Library UX | Partial | Verify subscribe, refresh, OPML import/export, iTunes search, show detail, and empty/error states on device and simulator. Remove remaining `SubscriptionService` compat paths once every view reads snapshots/dispatches actions directly. |
| PR 3 - Full player | Partial | Validate lock-screen metadata, remote commands, queue transitions, sleep timer, speed, position persistence, download-local playback, and AirPlay/route behavior. Fix any remaining iOS-side policy decisions. |
| PR 4 - Identity | Partial | Finish Rust-owned identity actions, raw pubkey projection, Keychain-backed credential replacement, NIP-46 pairing state, profile publishing, and removal of `UserIdentityStore` compat surfaces. |
| PR 5 - Downloads/auto-download | Partial | Rust now projects active/queued/paused/failed queue state and starts the next queued item from reports. Remaining: make offline-first playback explicit, validate background URLSession restore, and add deletion/auto-download regression coverage. |
| PR 6 - NIP-F4 | Partial/Scaffold | Wire tags and real pubkey derivation are corrected. Still implement persisted per-podcast secrets, signing, relay publish, relay-backed episode fetch, author claims, and deletion cleanup. See `docs/plan/pod0-nostr-publishing.md`. |
| PRs 7-N - AI/platform | Scaffold/Partial | Treat each merged surface as a starting point. Replace heuristics/placeholders with real provider, knowledge, relay, scheduling, and platform logic before marking any feature done. |

## Feature Parity Matrix

### Tier 1 - Core Podcast Player

| # | Feature | Status | Required Work |
|---|---|---|---|
| 1 | Subscribe via RSS feed URL | Partial | Validate feed fetch through `HttpCapability`, duplicate handling, malformed URL/error UI, persistence after restart, and Android behavior. |
| 2 | OPML import/export | Partial | Confirm large OPML import, partial failures, duplicate feeds, export fidelity, and no legacy `SubscriptionService` dependency. |
| 3 | Library/show grid | Partial | Ensure all legacy library states are represented from Rust snapshots: artwork, stale feeds, categories, played/bookmarked state, empty/error states. |
| 4 | Show detail + episode list | Partial | Finish snapshot-only navigation and remove any remaining compat/domain shims for show/episode lookup. |
| 5 | Feed refresh | Partial | Validate foreground/cold-start behavior, conditional GET, auto-download/new-episode hooks, and failure reporting. |
| 6 | Podcast search | Partial | Ensure iTunes search result mapping is Rust-owned, errors surface as data, and add tests for malformed provider responses. |
| 7 | Audio playback | Partial | Validate play/pause/seek/end-of-item/report loops on device; ensure no iOS-side queue/policy decisions remain. |
| 8 | Variable speed | Mostly done | Speed clamp matches 3.0x; still needs device validation through Rust action, audio executor, remote surfaces, and persisted settings. |
| 9 | Sleep timer | Partial | Confirm timer lifecycle, cancellation, background behavior, and sleep-report follow-up command path. |
| 10 | Episode download | Partial | Start/delete plus Rust progress/failed/paused projection exist; robust background restore, deletion failures, and offline-first playback validation remain. |
| 11 | Auto-download policy | Partial | Policy exists; validate after refresh, persistence, per-show UI, deletion interaction, and duplicate prevention. |
| 12 | Playback position persistence | Mostly done | Deterministic episode IDs and writeback exist; validate seek/resume after refresh, restart, and downloaded playback. |
| 13 | Playback queue | Partial | Queue actions/UI exist; validate item-ended advancement, persistence expectations, duplicate handling, reorder/remove semantics. |
| 14 | Lock-screen/control center | Partial | Metadata and remote commands need device validation and tests for current episode/artwork/skip interval changes. |
| 15 | Chapters | Partial | Fetch/display exists; validate Podcasting 2.0 JSON/VTT edge cases, persistence, and chapter seek behavior. |
| 16 | Mini player | Partial | Snapshot rendering exists; validate all now-playing transitions and remove any legacy playback state dependency. |
| 17 | Full player | Partial | UI exists; validate scrubber, speed, sleep, route picker, queue, chapters, transcript links, and accessibility. |
| 18 | Downloads manager | Partial | UI filters downloaded episodes and the kernel now exposes active/queued/paused/failed download state; offline playback, deletion failure handling, and empty/error states still need validation. |
| 19 | Playback settings | Partial | Some settings project/persist; finish OpenRouter/provider/settings surfaces and eliminate settings compat shims. |
| 20 | New-episode notifications | Partial | Notification command/capability exists; validate permission flow, quiet failure, dedup, deep link, and background delivery assumptions. |

### Tier 2 - Identity And Social

| # | Feature | Status | Required Work |
|---|---|---|---|
| 21 | Nostr keypair generation | Partial | Kernel/account projection exists; finish Rust-owned generate/import/clear actions and Keychain persistence without compat fallback. |
| 22 | BYOK/paste nsec | Partial | Replace `NostrCredentialStore`/`NostrKeyPair`/Bech32 shims with real keychain + `nmp-keys`/signer broker path. |
| 23 | NIP-46 remote signer | Partial | Broker wiring exists; finish live handshake, nostrconnect URI lifecycle, cancellation, error states, and account projection. |
| 24 | Profile editing + kind:0 publish | Partial | Current local `@AppStorage` fallback must become Rust/Nostr-owned profile publish plus relay confirmation/profile cache hydration. |
| 25 | NIP-65 relay list | Partial | UI exists; persist/read via NMP substrate relay-list store, publish real list, and remove `@AppStorage` seed fallback. |
| 26 | NIP-F4 discovery | Partial | Show search exists via HTTP gateway and parser tags are corrected; finish relay subscription path, episode fetch by podcast pubkey, and pure-Nostr subscribe. |
| 27 | NIP-F4 publish owned shows | Scaffold | Kind `10154` wire tags and pubkey derivation are corrected; persist the secret, sign the event, publish to relays, update author claims, and delete/cleanup owned-show state. |
| 28 | NIP-F4 publish episodes | Scaffold | Kind `54` wire tags are corrected; upload audio to Blossom, emit the canonical `audio` tag, sign the event, publish to relays, and project publish/queue errors. |
| 29 | Nostr episode comments | Scaffold | Replace `nostr_relay_pending` stubs with kind-1111 relay subscribe/publish and map local `EpisodeId` to NIP-73 podcast item anchors. |
| 30 | Friends/social graph | Scaffold | Replace `nostr_pending` stub with kind:3 contact-list store, metadata hydration, subscription refresh, and snapshot projection. |

### Tier 3 - AI Features

| # | Feature | Status | Required Work |
|---|---|---|---|
| 31 | AI inbox triage | Scaffold | Replace recency heuristic with provider-backed triage, persisted dismiss/listened state, explainable reasons, and failure handling. |
| 32 | AI agent chat | Scaffold | Replace canned assistant response with real LLM loop, tool execution, streaming/progress state, cancellation, memory/context policy. |
| 33 | Agent memory | Partial | Memory CRUD exists; integrate with agent prompt/tool loop, source attribution, persistence migration, and privacy controls. |
| 34 | Agent scheduled tasks | Scaffold | Replace run-now completion stamp with actual scheduler, task execution, notifications, persistence, and failure/retry policy. |
| 35 | Transcripts | Partial | Viewer and cache exist; wire multi-source transcript discovery/fetch/STT providers, persistence, search indexing, and failure states. |
| 36 | AI chapter compilation | Scaffold | Replace equal-length stub chapters with LLM/transcript-grounded chapters, persistence, regeneration, and provenance. |
| 37 | Auto ad skip | Partial | Segment model/player hook exists; add detector/source of ad segments, user controls, false-positive safeguards, and validation. |
| 38 | RAG/vector search | Scaffold | Replace substring ranker with `podcast-knowledge` embeddings/BM25, indexing jobs, scoped search, and result provenance. |
| 39 | AI wiki | Scaffold | Replace placeholder articles with RAG-backed synthesis, citations, refresh/invalidation, and per-podcast storage. |
| 40 | AutoSnip/clip composer | Partial | Clip UI/actions exist; add boundary refinement, persistence/export guarantees, share validation, and audio file handling. |
| 41 | AI briefings | Partial | LLM text/provider leg shipped (M5.6 `briefing_llm.rs`); still open: scheduler trigger, structured composer (intro→summaries→outro via `podcast-briefings::SegmentKind`), TTS/audio generation, persistence, and failure/retry projection. Blocked on reconciling the two briefing mechanisms (`tasks_handler` seed vs unwired `podcast-briefings::BriefingScheduler`) — see BACKLOG `briefings-real-pipeline`. |
| 42 | Voice mode | Partial | iOS STT/TTS exists; finish Rust conversation manager, barge-in policy, provider TTS/STT choices, audio-session state, and transcript handoff. |
| 43 | Agent-generated TTS podcasts | Partial | The capability ships behind the **Swift agent tool** (`generate_tts_episode` → `AgentTTSComposer.generateAndPublish`): ElevenLabs synth → stitched m4a in Application Support (`AgentGeneratedPodcastService.audioFileURL`) → real `Episode` published on the "Agent Generated" virtual podcast, with transcript + chapters persisted. So media persistence + show/episode publishing already exist. The two-mechanism reconciliation is **resolved (Option A)**: the orphaned kernel `podcast.tts` path (`tts.rs`/`tts_llm.rs`/`TtsEpisodeSummary`) was deleted in `feat/m9-delete-tts-stub`, leaving the Swift composer as the single TTS mechanism. Remaining follow-ups (NIP-F4 publishing, deletion cleanup, restart round-trip of `.synthetic` episode metadata) are tracked on the Swift path — see BACKLOG `tts-episodes-reconcile-two-mechanisms`. |
| 44 | Nostr agent-to-agent | Blocked/Scaffold | Needs real identity, signer, relay, contact/trust, and kind:1 notes threaded via NIP-10 before UI can be real. NIP-17 is an explicit non-goal. |
| 45 | Agent categorization | Partial | **Rust path complete:** two-phase categorization (`categorization.rs`) — a synchronous keyword pass gives instant tags, then a background LLM pass (`categorization_llm.rs`, Ollama, fixed 15-item taxonomy with off-list filtering) re-stamps each episode. Runs automatically for all new episodes via `auto_categorize()` after subscribe / refresh / refresh-all. Labels are keyed by the deterministic UUIDv5(`feed_url`,`guid`) episode id, so they survive `merge_episodes` across refreshes. Per-episode `ai_categories` and the rolled-up `CategoryBrowseItem` aggregate both project into the iOS snapshot (`snapshot_categories.rs`). "Embedding-backed" reads as LLM-backed in this Ollama app (key principle). **Render gap (like #43):** iOS decodes `update.categories` / `episode.aiCategories` in the bridge (`KernelModelHashing.swift`) but **no SwiftUI view renders them** — the Settings "Categories" surfaces render a separate Swift user-defined category model (`AppStateStore+Categories`, keyed by `subscriptionIDs`), not the AI categorization output. So the AI categorization is not yet user-visible through the NMP stack. **Decision pending:** wire the AI `CategoryBrowseItem` / `aiCategories` into an iOS browse/episode-tag surface, or delete the orphaned Rust projection (cf. #43's delete-the-stub resolution). Not done either way: user correction loop + localization. |
| 46 | AI agent picks | Partial | **Rust path complete + personalized:** two-stage picks rail (`picks_handler.rs`) — the newest-first heuristic (`compute_picks`, per-show diversity cap + total limit) stamps the slot immediately, then a background LLM pass (`picks_llm.rs`, Ollama) re-ranks with `compute_picks_scored`. Ranking is personalized: `build_listening_profile` summarizes the user's actually-engaged shows from `played` / in-progress (`position_secs`) / `is_starred` signals and conditions the scoring prompt on it (cold-start degrades to general interest). Each pick carries an explainable `pick_reason`. The LLM path now runs automatically after every feed refresh via `auto_refresh_picks()` (re-entrancy-guarded) and on explicit `podcast.picks.refresh`; picks persist in the `PodcastHandle` slot and project into the iOS snapshot (`update.picks`). "Personalized ranking" reads as LLM-backed in this Ollama app (key principle). **Render gap (like #43):** iOS decodes/hashes `update.picks` (`KernelModelHashing.swift`) but **no SwiftUI view renders it** — the Home Featured rail was deliberately migrated to render Inbox-triage decisions (`HomeInboxBundleBuilder`, replacing the old `AgentPicksService`, which is now dead with zero render consumers). So the Rust picks slot is not user-visible through the NMP stack. **Decision pending:** cut the Home Featured rail (or a new rail) over to `update.picks`, or delete the orphaned Rust slot (cf. #43). Not done either way: explicit opt-out/reset control. |

### Tier 4 - Platform Integration

| # | Feature | Status | Required Work |
|---|---|---|---|
| 47 | CarPlay | Partial | Library/playback templates exist; validate on simulator/head unit, now-playing sync, entitlement behavior, and cold-connect placeholder. |
| 48 | Widgets/Live Activity | Partial | Live Activity exists; wire durable widget snapshot from kernel/codegen, validate activity lifecycle and App Group data. |
| 49 | AppIntents/Siri | Partial | Active App target now compiles voice plus Pause/Resume/Skip shortcuts through `NotificationCenter`; Play Latest is intentionally hidden until it can route through the Rust-owned `podcast.siri.play_latest` action. Validate Siri/Spotlight phrases, background behavior, unavailable playback state, localized phrases, and reconcile the Swift bridge with the Rust-owned policy path before marking done. |
| 50 | Spotlight indexing | Partial | Indexing exists; validate throttling, deletion/update behavior, deep links, and no playback-position reindex churn. |
| 51 | Handoff | Partial | NSUserActivity donation exists; validate continue path, stale activity invalidation, and cross-device behavior. |
| 52 | iCloud settings sync | Partial | KVS bridge exists; confirm Rust owns settings policy, conflict handling, opt-in/availability, and echo suppression. |
| 53 | Local notifications | Partial | Capability exists; validate authorization, scheduling, taps/deep links, duplicate prevention, and quiet hours if required. |
| 54 | Android second-platform | Partial | Compose shell and ExoPlayer work started; finish real snapshot parity, auth/keychain, gradle wrapper, audio report path, and Tier 1 flows. |

## Scaffold Burn-Down Rules

- A scaffold is not done because it has a screen, projection, or action enum.
- Every scaffold must have one backlog item with: owner surface, current fake behavior, required real behavior, tests, and deletion criteria for temporary code.
- Do not add new feature surfaces until the P0 protocol/compat/validation debt is actively shrinking.
- When converting a scaffold, remove user-visible copy that implies the feature is complete unless the real backend works.
- Every user-facing iPhone behavior change needs a `whats-new.json` entry.

## Immediate Priority Order

1. NIP-F4 secret persistence, signing, relay publish/discovery, and author claims.
2. iOS validation gate: broaden Swift test coverage now that the focused NIP-46 test and AppIntents compile blockers are cleared.
3. Remaining compat shims and identity/settings ownership.
4. Capability push/routing cleanup and validation gate.
5. Tier 1 device-level usability validation.
6. AI scaffolds: agent chat, RAG, wiki, briefings, voice, TTS, inbox, tasks.
7. Platform hardening: CarPlay, Live Activity/widget, AppIntents, Spotlight, Handoff, iCloud, notifications.
8. Android Tier 1 parity and contributor build setup.
9. Delete `App/Sources/` only after all exit criteria pass.

## Exit Criteria For Feature Parity

1. Every feature above is `Done`.
2. `App/Sources/` can be deleted without breaking any user flow.
3. iOS and Android ship from the same Rust kernel build.
4. No compat stub files remain in `ios/Podcast/Podcast/Compat/`.
5. All NIP-F4 publish/discovery paths produce and consume canonical NIP-F4 event shapes.
6. `cargo test --workspace`, focused `xcodebuild test`, and the full merge gate pass.
7. `docs/plan.md` and `docs/BACKLOG.md` agree with the code state.
