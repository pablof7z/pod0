# Backlog

This is the tactical queue for active work, follow-ups, and pending decisions.
Do not duplicate these items in `WIP.md`; `WIP.md` only records branches and
worktrees currently in flight.

## Active P0 - Correctness Before More Features

- ~~**p0-nipf4-wire-contract.**~~ Done in PR #89: aligned kind `10154`/`54`
  builders and parsers with the NIP-F4 wire contract; removed non-NIP-F4
  `d`/`a`/`published_at`/`imeta` tags; round-trip tests verify absence.
- **p0-nipf4-real-keys.** ~~Real pubkey derivation~~ done in PR #93
  (`nostr::Keys::generate()` + real secp256k1). Remaining: persisted storage,
  Keychain-backed secret, survive restart, cleanup on owned-podcast delete.
- **p0-nipf4-sign-and-publish.** Replace unsigned `event_json` plus
  `relay_pending` diagnostics with signed events published to configured
  relays. If publish is async, implement a durable queue with retry/error
  projection; do not imply success before relay acceptance or durable queueing.
- **p0-nipf4-relay-discovery.** Implement canonical relay-backed kind `10154`
  show discovery and kind `54` episode fetch by podcast pubkey. Gateway search
  may remain as an optional convenience, not the source of truth.
- **p0-nipf4-author-claim.** Publish and refresh kind `10064` author claims
  after owned-podcast create/update/delete. Tests must verify exact `p` tags
  and signer identity.
- **p0-plan-truthfulness.** Keep `docs/plan.md`,
  `docs/plan/nmp-feature-parity.md`, and this backlog synchronized with code.
  Do not mark scaffolded behavior done.
- **p0-validation-gate.** Establish the merge gate for this migration:
  `git diff --check`, focused Rust tests for touched crates,
  focused Swift/iOS tests for touched targets, and full-suite validation before
  declaring feature parity.
- ~~**p0-ios-test-target-compile.**~~ Fixed across PR #101 and PR #102:
  `Nip46RemoteSignerTests.swift` now accepts an optional bunker pubkey, the
  active Tuist target no longer references the dead `KernelModel` duplicate,
  and `App/Sources/AppIntents/PlaybackAppIntents.swift` uses a Notification
  bridge that compiles in the `Podcastr` target. Remaining shortcut hardening
  stays tracked under `appintents-validation`.

## Active P1 - Compat And Ownership Burn-Down

- **compat-service-stubs-delete.** Delete remaining
  `ios/Podcast/Podcast/Compat/ServiceStubs.swift` sections by replacing them
  with Rust-backed actions/snapshots or real capabilities: BYOK connect,
  subscription service, LiquidGlassSegmentedPicker shim if still needed,
  `NostrCredentialStore`, `NostrKeyPair`, NIP-46 connect card, and agent
  connection settings.
- **compat-domain-stubs-delete.** Delete
  `ios/Podcast/Podcast/Compat/DomainStubs.swift` by routing every migrated
  view through generated snapshot types and real Rust domain projections.
- **compat-kernelmodel-delete.** Delete `KernelModelCompat.swift` by replacing
  convenience lookups and no-op agent/social methods with canonical snapshot
  queries and Rust actions.
- **compat-useridentity-delete.** Delete `UserIdentityStoreCompat.swift` after
  identity import/generate/clear/profile/NIP-46 flows are fully Rust/NMP-owned.
- **identity-kernel-actions.** Implement Rust-owned identity actions:
  import nsec, generate, clear, publish profile, connect/disconnect remote
  signer, connect via nostrconnect, cancel handshake, and expose raw hex pubkey
  plus fingerprint data in `AccountSummary`.
- **settings-provider-ownership.** Move OpenRouter mode, BYOK-imported
  credentials metadata, provider settings, and onboarding gate decisions into
  Rust-owned settings projections/actions. Delete Keychain-only UI fallbacks
  once the kernel can represent the state.
- **relay-list-ownership.** Replace `@AppStorage("nip65.relays")` seed state
  with NMP relay-list store reads/writes and real NIP-65 publish/refresh flow.
- **snapshot-push-delivery.** Replace the remaining 500 ms polling dependency
  with push-style delivery through the NMP update sink for autonomous changes,
  while keeping content-hash throttling for volatile playback/download fields.
- **capability-router-unify.** Collapse `SyncCapabilityBridge` and
  `PodcastCapabilities.shared.handleJSON()` into one routing contract. Each
  capability owns its threading; Rust has one path to dispatch commands and
  receive reports.

## Active P1 - Tier 1 Usability Hardening

- **bunker-isconnecting-reactive.** `RemoteSignerView.connect()` sets
  `isConnecting = false` immediately after dispatching `signInBunker` (fire-
  and-forget). Should clear when `activeAccount` appears in snapshot (or on a
  timeout), so the spinner stays up while the NIP-46 handshake is in flight.
  Requires `bunkerHandshake` state surfaced in `IdentityViewModel`.
- **rss-subscribe-validation.** Validate malformed URLs, duplicate feeds,
  provider errors, restart persistence, Android behavior, and empty/error UI.
- **opml-import-export-hardening.** Validate large OPML files, partial
  failures, duplicate feeds, export fidelity, and no legacy subscription
  service dependency.
- **feed-refresh-hardening.** Validate cold start, foreground refresh,
  conditional GET, failure reporting, notification hooks, and auto-download
  hooks.
- **player-device-validation.** Validate play/pause/seek/speed/sleep/end item,
  lock-screen metadata, remote commands, route changes, AirPlay, and background
  behavior on simulator and device.
- **queue-hardening.** Validate item-ended advancement, duplicate handling,
  remove/clear, persistence expectations, and UI sync.
- **download-state-projection.** Runtime queue projection is now wired:
  player download actions mutate `DownloadQueue`, download reports update
  progress/paused/failed/completed state, and snapshots expose active/queued/
  paused/failed rows instead of only completed local paths. Remaining:
  validate background URLSession restore, deletion failure, and offline-first
  playback on device.
- **settings-completion.** Finish playback/settings projection parity:
  skip intervals, auto-skip ads, streaming/offline preferences, onboarding
  gate, provider settings, and persistence migration.
- **notification-hardening.** Validate authorization, schedule/update/cancel,
  deep links, duplicate prevention, and quiet failure behavior.

## Active P1 - Social/Nostr Real Logic

- **episode-comments-relay-wiring.** Replace `comments_handler.rs` stubs with
  real kind-1111 relay subscribe/publish. Map local `EpisodeId` to
  Podcasting 2.0 guid/NIP-73 `i podcast:item:guid:<guid>` anchors.
- **social-graph-store-wiring.** Replace `social_handler.rs` `nostr_pending`
  with NMP kind:3 contact-list store reads, kind:0 metadata hydration,
  subscription refresh, and snapshot updates.
- **nostr-conversations-real-projection.** Replace compat-empty
  conversation/approval surfaces with Rust-owned conversation projection,
  trust-list/approval actions, kind:0 profile cache, and NIP-46
  integration.
- **agent-to-agent-kind1.** Implement agent-to-agent messaging via public
  kind:1 notes threaded with NIP-10 (`e`/`p` tags for reply chains) only
  after identity, signer, relay, contact, and trust-list primitives are real.
  Non-goal: NIP-17 (private direct messages) is out of scope for agent
  coordination and will not be used for this purpose.

## Active P1 - AI Scaffold Replacement

- **agent-chat-real-loop.** Replace canned assistant responses with real LLM
  streaming, tool execution, progress/cancel states, memory/context policy,
  provider errors, and transcripted tool results.
- **rag-vector-search-real.** Replace substring search with
  `podcast-knowledge` indexing, embeddings, BM25/KNN retrieval, scoped
  search, provenance, and reindex jobs.
- **wiki-real-generation.** Replace placeholder wiki articles with RAG-backed
  synthesis, citations, refresh/invalidation, per-podcast storage, and delete
  semantics.
- **briefings-real-pipeline.** Replace generating placeholder with scheduler,
  content selection, script generation, TTS/audio output, playback handoff,
  persistence, and failure/retry projection.
- **voice-real-manager.** Finish Rust voice conversation manager, audio-session
  state transitions, barge-in policy, provider TTS/STT choices, transcript
  handoff, and cancellation.
- **tts-episodes-real-generation.** Replace placeholder scripts with real
  provider-generated audio, persisted media, playback, deletion, and optional
  NIP-F4 publishing integration.
- **ai-chapters-real-generation.** Replace equal-length stub chapters with
  transcript/LLM-grounded chapters, provenance, regeneration/clear behavior,
  and persistence.
- **inbox-triage-real-model.** Replace recency heuristic with provider-backed
  triage, persisted dismiss/listened state, explainable reasons, and user
  correction loop. Partially done in PR #123 (rig-core + Ollama LLM scoring
  wired; remaining items below).
- **inbox-triage-async-streaming.** Move `run_llm_triage` off the actor thread
  into a background Tokio task that streams scored results back incrementally
  via the rev counter. Currently the actor thread blocks for N×LLM latency
  while episodes are triaged sequentially; this must be fixed before triage
  is triggered automatically (not just on explicit user action).
- **inbox-triage-cache-persist.** Persist `inbox_triage_cache`
  (`HashMap<String, TriageResult>`) to disk alongside the podcast store so
  cold launches do not re-triage every episode. Use the existing data-dir
  path convention; serialize as JSON; reload on `set_data_dir`; invalidate
  stale entries when episode metadata changes.
- **agent-tasks-real-scheduler.** Replace run-now completion stamps with
  actual scheduling, task execution, notifications, persistence, and retries.
- **agent-picks-real-ranking.** Replace newest-first heuristic with
  personalized ranking, explainable reasons, refresh policy, and opt-out/reset.
- **categorization-real-model.** Replace keyword classification with
  provider/embedding-backed categorization, corrections, persistence, and
  localization.
- **autosnip-real-boundaries.** Add boundary refinement, clip persistence,
  export/share guarantees, and media-file handling.
- **agent-memory-integration.** Wire memory CRUD into the real agent prompt/tool
  loop with source attribution, privacy controls, and migration behavior.

## Active P1 - Platform And Android

- **platform-widget-snapshot-codegen.** Replace hand-mirrored widget/live
  activity payloads with generated projection types and Rust-owned widget
  snapshots.
- **carplay-validation.** Validate templates, now-playing sync, entitlement
  behavior, cold-connect placeholder, and playback dispatch on CarPlay
  simulator/head unit.
- **appintents-validation.** Validate Siri/Spotlight phrases, unavailable
  playback state behavior, localized phrases, background execution, and
  reconcile the active App target's Notification bridge with Rust-owned
  playback policy. Reintroduce Play Latest only after the active app can route
  it through `podcast.siri.play_latest` instead of selecting episodes in Swift.
- **spotlight-hardening.** Validate indexing throttles, deletion/update,
  deep links, and no reindex churn from playback-position ticks.
- **handoff-hardening.** Validate NSUserActivity donation/invalidation,
  continue path, and stale activity behavior across devices.
- **icloud-settings-hardening.** Confirm Rust owns settings policy, conflicts,
  echo suppression, availability, opt-in behavior, and migration.
- **android-tier1-parity.** Finish Android real snapshot parity for library,
  player, downloads, identity, feed refresh, and audio reports.
- **android-gradle-wrapper.** Vendor `gradlew` and wrapper files under
  `android/Podcast/`.
- **android-auth-keychain.** Replace Android `signinNsec` stub with a real
  secure-storage identity sheet mirroring iOS.

## Active P2 - Cross-Cutting Technical Debt

- **m5-non-utf8-feed-bodies.** Widen HTTP capability body transfer to preserve
  non-UTF8 feed bytes. Update Swift and Rust so XML encoding declarations are
  honored.
- **m5-chirp-headers-parity.** Reconcile podcast-player and Chirp HTTP header
  schemas once the canonical `nmp-core::capability::http` shape lands.
- **legacy-app-deletion-gate.** Do not delete `App/Sources/` until every
  feature in `docs/plan/nmp-feature-parity.md` is `Done` and the NMP app is
  the sole implementation for user flows.
- **whats-new-audit.** Every user-facing iPhone change must add a unique
  `shipped_at` entry in `App/Resources/whats-new.json` and the mirrored iOS
  resource if still required by project shape.
- **docs-status-audit.** Every PR that changes a listed item must edit the
  existing backlog item instead of adding parallel state or leaving stale
  status behind.
- **line-limit-audit.** Continue enforcing the 300-line soft and 500-line hard
  limits. Split files before adding logic to near-limit modules.

## Pending Decisions

- **storage-engine-canonicality.** The old plan called for sled; the current
  implementation uses JSON persistence for `PodcastStore`. Decide whether JSON
  is the accepted canonical storage for the current milestone or whether a
  sled/SQLite migration is required before parity.
- **Relay publish queue semantics.** Decide whether relay publish is
  synchronous user-visible completion or durable async queue with retry and
  status projection.
- **Provider availability.** Decide which AI/STT/TTS/provider features are
  user-visible without configured credentials and what disabled/error state
  each surface should show.

## Done / Recently Reconciled

- **pod0-rename.** Done via PR #52; visible app name is Pod0 while stable
  identifiers remain unchanged.
- **episode-id-stability.** Done via PR #70; `EpisodeId::from_feed_and_guid`
  derives deterministic IDs and feed refreshes no longer break local paths or
  position lookup.
- **speed-chip-clamp-mismatch.** Done via PR #55; Rust and iOS speed clamps
  allow up to 3.0x.
- **appintents-siri-rust-policy.** Done via PR #87; Siri play/latest/resume
  policy moved to Rust actions.
- **episode-description-htmlstrip.** Done via PR #87; descriptions are stripped
  at Rust projection time.
- **nipf4-wire-contract.** Done via PR #89; kind `10154`/`54` builders and
  parsers conform to the NIP-F4 wire contract; non-NIP-F4
  `d`/`a`/`published_at`/`imeta` tags are no longer emitted or required.
- **nipf4-real-pubkey-derivation.** Done via PR #93; `PodcastKeyStore` now uses
  real secp256k1 key generation/public-key derivation. Persisted secret storage
  remains tracked under `p0-nipf4-real-keys`.
- **home-inbox-status-line.** Done via PR #94; the Home Inbox header reports
  triage freshness and kept/archived counts.
- **ollama-local-provider.** Done via PR #95; local/self-hosted Ollama
  endpoints can omit API keys and model discovery uses the configured host.
- **playback-restore-auto-download.** Done via PR #96; restoring the last
  played episode no longer starts the background download pipeline until the
  user presses play.
- **file-size-initial-splits.** Done for the known projection/store/test/action
  overages identified before this pass; continue auditing new changes.
- **wip-reconciliation.** Done for 2026-05-26; `WIP.md` is the live source for
  active worktrees, stale PR-stack entries were removed, and it should return
  to `Active` = `_None._` after each agent-owned PR merges.
