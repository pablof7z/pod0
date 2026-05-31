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
- **player-actor-queue-unification.** `maybe_auto_advance` now pops from the
  canonical `PlaybackQueue` (`handle.queue`, the queue the UI enqueues into via
  `podcast.queue` and the snapshot renders). The separate `PlayerActor.queue`
  (populated only by the `podcast.player` `Enqueue`/`PlayNext` ops, which the UI
  does not use) is now vestigial for auto-advance. Collapse the two queues into
  one owner: route the `podcast.player` enqueue ops at `PlaybackQueue` (or delete
  them) and drop `PlayerActor`'s queue field + `enqueue`/`pop_next`/`queue()`.
- **remote-command-kernel-routing.** Lock-screen / Control Center commands
  (`AudioCapability+RemoteCommands`) call `execute(.play)`/`.seek` which run the
  engine directly through the same `commandHandler` that Rust-issued commands
  use. After a cold restart where the player restored a paused episode but Rust
  never staged it, a lock-screen Play starts audio without a `kernelLoad`, so
  Rust has no `episode_id` for the subsequent position reports. Fix by routing
  lock-screen-originated commands through a report-to-Rust path (or staging the
  restored episode in Rust on restore) — distinct from Rust-issued playback
  commands so it doesn't loop through `handle_load`'s echoed `Load`.
- **carplay-chapters-live-resolve.** `CarPlayNowPlaying` reads
  `playback.episode.chapters` directly; when chapters hydrate after the episode
  loaded (or before CarPlay connects), `PlaybackState.episode` can be the stale
  pre-hydration copy. Restore a store-backed resolver so the CarPlay chapter
  button/list appears once the store has chapters.
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
- **stale-subscription-refresh-test.** `SubscriptionRefreshServiceTests`
  (`testSubscriptionServiceRefreshUsesSharedRefreshSemantics`) injects a Swift
  `FeedClient(session:)` stub, but `SubscriptionService.refresh` now delegates to
  `store.kernelRefresh` (Rust), which fetches via its own HTTP capability and
  ignores the injected client — so the stubbed feed never reaches the kernel and
  the assertions (Fresh Title / etag / episode-1) fail. Pre-existing on main
  (test last touched by PR #131, before refresh moved to the kernel). Rewrite to
  stub the Rust HTTP capability (or move to a headless scenario) or delete; it no
  longer exercises the live path.

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
- **agent-to-agent-kind1 (feature #44).** Agent-to-agent messaging over
  public kind:1 notes threaded with NIP-10.
  - **DONE — raw transport.** `agent_note_handler.rs` (PR for #44) signs +
    publishes kind:1 notes addressed to a peer (`["p",<hex>]`) with the
    NIP-10 root marker `["e",<root>,"","root"]` via the `PublishAgentNote`
    action (returns `{status:"published"|"signed","event_id"}`), and
    subscribes `{kinds:[1],"#p":[me]}` via `FetchAgentNotes`, parsing inbound
    notes into `AgentNoteSummary` and projecting them onto
    `PodcastUpdate.agent_notes` (reactive push seam). Self-authored notes are
    filtered. Unit + headless (`scenarios/agent_notes.rs`) coverage exists.
    `AgentNoteSummary`/`agent_notes` is an **interim flat projection**: the
    canonical conversation model already exists as
    `podcast-agent-core::ConversationActor` / `NostrConversation` and the
    Swift consumer (`NostrConversationsView`) binds conversation-shaped
    `nostrConversations` keyed by `rootEventID`. The flat note list is
    expected to be **subsumed/replaced** by the Rust-owned conversation
    projection under `nostr-conversations-real-projection`; it is shipped
    now only to give the kind:1 transport an observable output seam.
  - **OPEN — trust gate.** Every inbound note is projected `trusted:false`;
    the Rust side cannot classify a sender as an approved peer until the
    kind:3 contact list + trust list are real (`social-graph-store-wiring`,
    `nostr-conversations-real-projection`). The iOS shell must route inbound
    notes to an approval surface and must not auto-respond until then.
  - **OPEN — LLM responder loop.** The inbound→model→outbound autopilot
    (dedup via responded-event ids, per-root outgoing turn cap, `wtd-end`
    end-conversation gate, bounded kind:0 profile hydration, owner-consult
    `ask` tool) still lives on the Swift `NostrAgentResponder` path. Porting
    it to the kernel depends on the trust gate landing first.
  - Non-goal: NIP-17 (private direct messages) is out of scope for agent
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
- **briefings-real-pipeline (feature #41).** The matrix's definition of done is
  six components: scheduler, composer, provider pipeline, audio generation,
  persistence, and playback handoff. Status decomposed (verified against code on
  main, 2026-05-31):
  - **DONE — text generation (M5.6, `feat/m5-briefings`).** `briefing_llm.rs`
    asks Ollama (`deepseek-v4-flash`) for a 3–5 item summary over the top-10
    recent unplayed episodes; `briefings_handler::handle_generate_briefing`
    flips the slot `generating → ready`, with a no-LLM `fallback_segments`
    safety net. This is the *provider/script-generation* leg only.
  - **OPEN — scheduler.** Nothing auto-fires on a schedule. The `tasks_handler`
    seed already has a "Morning Briefing" row (`schedule:"daily"`,
    op `generate_briefing`), but no ticker/cron consumes `schedule`/`next_run_at`
    (both are always `None`; only `RunNow` — a manual button — dispatches). The
    `podcast_briefings::BriefingScheduler` state machine (`should_generate_now`,
    `start_pending`, `next_scheduled_minutes`) is fully implemented **but
    instantiated nowhere** outside its own crate.
  - **OPEN — composer.** `handle_generate_briefing` emits flat
    `BriefingSegmentSummary{kind:"episode_summary"}` rows only. The canonical
    `podcast_briefings::SegmentKind` (`Intro`/`EpisodeSummary`/
    `NewEpisodeAlert`/`WeatherUpdate`/`OutroCallToAction`) and `BriefingSegment`
    domain types exist but are unused on the live path. Per the crate's D7
    doctrine, intro→summaries→outro structuring belongs in `podcast-briefings`,
    not in `nmp-app-podcast`'s handler.
  - **OPEN — audio generation.** Briefings produce text only; no TTS/audio
    output. (TTS now lives only in the Swift `AgentTTSComposer` path — the
    feature-#43 kernel `tts.rs`/`tts_llm.rs` stub was deleted; see
    `tts-episodes-reconcile-two-mechanisms`. Briefing audio would compose
    against the Swift path or a new capability, not the removed kernel stub.)
  - **OPEN — persistence + failure/retry projection.** The slot is an in-memory
    `Arc<Mutex<Option<BriefingSnapshot>>>`; nothing survives restart and
    `status` never reaches `"failed"` on the live path.
  - **BLOCKED ON A DECISION (do not implement piecemeal):** there are two
    parallel briefing mechanisms — the `tasks_handler` seed-task path (World A:
    `BriefingSnapshot`/`BriefingSegmentSummary` projection types) and the
    unwired `podcast-briefings::BriefingScheduler` + `Briefing`/`BriefingSegment`
    domain crate (World B, the M9.A skeleton that explicitly defers
    composer/stitcher/audio to M9.B–C). Scheduler, composer, audio, persistence,
    and failure projection all pull on reconciling these two. **A human must
    decide which mechanism is canonical** (wire `BriefingScheduler` into the
    kernel as SSOT, or extend the `tasks_handler` path and retire the unwired
    crate) before an agent builds the remaining legs — building any one leg on
    the live handler alone would add a second composition path (the
    fragmentation AGENTS.md forbids) and a D7 violation. Sequenced to M9.B per
    the `podcast-briefings` skeleton.
- **voice-real-manager.** Finish Rust voice conversation manager, audio-session
  state transitions, transcript handoff, and cancellation. (Provider TTS/STT
  choices and barge-in policy are tracked separately — see
  voice-provider-selection below for the feature #42 provider blockers.)
- **voice-provider-selection (feature #42).** Investigated for M5 voice-mode
  provider wiring; the conclusion is that the M3 provider *settings* exist but
  there is no non-native *execution path* to dispatch them to, so the settings
  cannot be wired without net-new work that exceeds "wire an existing setting".
  Concrete blockers, in dependency order:
  1. **No provider field on the voice wire schema.** `VoiceCommand::Speak` /
     `StartListening` (`apps/nmp-app-podcast/src/capability/voice.rs` +
     `App/Sources/Capabilities/VoiceCapability+Wire.swift`) carry no provider
     selector. iOS `VoiceCapability.speak` maps `voice_id` directly to
     `AVSpeechSynthesisVoice(identifier:)`/`(language:)`, so passing an
     ElevenLabs voice id (e.g. `eleven_labs_voice_id`) through the existing
     `voice_id` field resolves to `nil` → silent fall-back to the default
     device voice. Do NOT wire `eleven_labs_voice_id` into `Speak.voice_id`;
     it looks done and does nothing. A `provider` field (plus a provider-
     scoped voice id) must be added to the schema on both sides first.
  2. **No iOS provider adapter exists.** The `Capabilities/Tts/{ElevenLabsAdapter,
     AvSpeechAdapter}` referenced in the `voice.rs` doc comment are unwritten;
     iOS is `SFSpeechRecognizer` (STT) + `AVSpeechSynthesizer` (TTS) only.
     There is no ElevenLabs WS/HTTP TTS client, no AssemblyAI STT client, and
     no OpenRouter Whisper STT client anywhere in Rust or Swift — every
     `eleven_labs`/`assembly_ai`/`whisper` reference is settings/iCloud-sync/
     Keychain plumbing, not a call site. A provider-routed TTS path also needs
     an audio sink (the synthesized bytes must reach `nmp.audio.capability` or
     the OS audio engine).
  3. **Two referenced settings do not exist.** There is no `tts_provider`
     selector (TTS provider is implicit-from-ElevenLabs-config) and no
     barge-in-threshold setting, and no OpenRouter-TTS-voice setting. The M5
     task forbids adding settings, so barge-in policy and OpenRouter TTS stay
     deferred until those fields are designed.
  4. **Default-vs-execution mismatch — resolved in PR #178.** Previously
     `stt_provider` defaulted to `"elevenlabs_scribe"` (`store/settings.rs`) and
     was projected verbatim into `SettingsSnapshot.stt_provider`, so the snapshot
     reported a non-native STT provider while the app actually transcribed
     on-device with `SFSpeechRecognizer` — a user reading settings believed they
     were on Scribe when they were not. An earlier draft of this entry warned
     "do NOT fix by changing the default" on the assumption it would break the
     iOS settings-UI contract. PR #178 deliberately made that change and accepted
     the tradeoff: the default flipped from `elevenlabs_scribe` to `apple_native`
     across the Rust store, projection, typed core Settings, and the Swift mirror,
     because `elevenlabs_scribe` required an API key the user may not have and the
     auto-ingest gate then stranded keyless users with no transcription. Apple
     on-device needs no key. A runtime fallback in
     `TranscriptIngestService.effectiveSTTProvider` also downgrades a keyless
     cloud provider to `.appleNative`, so the reported provider now matches the
     engine that actually runs (cloud transcription is preserved when a key is
     present). When the AssemblyAI/Scribe STT execution path lands, revisit
     whether a cloud default is appropriate for key-configured users.
  Suggested landing order once unblocked: (1) add `provider` + provider-scoped
  voice to the wire schema; (2) iOS ElevenLabs TTS adapter + audio sink behind
  that provider; (3) route `eleven_labs_voice_id`/`eleven_labs_tts_model` in
  `VoiceConversationManager` (D7: Rust decides voice) only when the active
  provider can honor it; (4) AssemblyAI/Scribe STT execution path + reconcile
  the cloud `stt_provider` default for key-configured users (the keyless
  `apple_native` default already landed in PR #178); (5) design
  barge-in-threshold + OpenRouter-TTS
  settings, then wire barge-in and OpenRouter TTS.
- **voice-mode-elevenlabs-tts-playback-sink.** The kernel-driven voice executor
  (`VoiceCapability.speak`) now *routes* on the projected `eleven_labs_voice_id`
  (set ⇒ user chose ElevenLabs TTS) but falls back to AVSpeech with an honest
  log line because there is no audio playback sink in this path:
  `ElevenLabsTTSClient.synthesizeStream` yields raw audio `Data` frames and the
  only consumer (`AudioConversationManager.beginSpeaking`) records them for
  barge-in and marks playback "future work" — no `AVAudioPlayerNode` route is
  wired through `AudioCapability`. To make ElevenLabs TTS audible in the
  kernel-driven path: add a player-node sink (likely via `AudioCapability`),
  feed `ElevenLabsTTSClient` frames into it, and emit `started`/`finished`
  `VoiceReport`s from real playback callbacks. Until then the fallback is the
  correct behavior. Note: this is separate from the parallel SwiftUI
  `AudioConversationManager` voice path used by `VoiceView`, which has the same
  missing-sink gap.
- **tts-episodes-reconcile-two-mechanisms (feature #43) — RESOLVED.**
  **Option A chosen — kernel stub deleted, Swift `AgentTTSComposer` is
  canonical.** The orphaned kernel `podcast.tts` vertical (`tts.rs`,
  `tts_llm.rs`, `TtsEpisodeModule`/`TtsEpisodeAction`, the `TtsEpisodeSummary`
  projection + `PodcastUpdate.tts_episodes` snapshot leg, the in-memory
  `tts_episodes` slot, and their tests) was removed in `feat/m9-delete-tts-stub`.
  The Swift agent-tool path (`generate_tts_episode` → `AgentTTSComposer`) is now
  the single TTS mechanism. Rust-only change: the Swift Bridge mirror
  (`ttsEpisodes` / `TtsEpisodeSummary`) decodes the now-always-absent JSON field
  via `decodeIfPresent ?? []`, so the iOS build is unaffected (the leftover
  mirror is harmless dead code, sweepable when the codegen pipeline lands).
  Untouched: all `eleven_labs_*` voice settings, `capability/voice.rs`,
  `VoiceCommand`, and the voice-conversation path — that is the live
  ElevenLabs/AVSpeech TTS capability, distinct from the deleted episode stub.

  Historical investigation (retained for context):
  Investigated for M9 ("media persistence + show/episode publishing"). Finding:
  those legs were NOT missing — they already ship, but behind a *different
  mechanism* than the one the matrix row tracked. There were two parallel,
  disconnected TTS paths:
  1. **Swift agent-tool path (the real, complete capability).**
     `AgentTools+TTS.generate_tts_episode` → `AgentTTSComposer.generateAndPublish`
     does ElevenLabs synthesis → stitched m4a written to Application Support
     (`AgentGeneratedPodcastService.audioFileURL`, `agent-episodes/<id>.m4a`) →
     publishes a real `Episode` on the "Agent Generated" virtual podcast
     (`AgentGeneratedPodcastService.publishEpisode`) → persists transcript +
     chapters. Media persistence (the m4a is written to durable Application
     Support) and show/episode publishing integration (a real `Episode` is
     upserted onto a `Podcast(kind: .synthetic)`) both exist here. NOT audited
     in this investigation, and still potentially open on this path: NIP-F4
     publishing of these episodes, deletion cleanup (no removal of the
     `agent-episodes/<id>.m4a` file or its store entry was found), and whether
     the published `.synthetic` episode *metadata* round-trips the store's disk
     layer so the episode reappears after restart (the audio file persists
     regardless; the library entry is unverified).
  2. **Kernel `podcast.tts` path (orphaned scaffold).** `tts.rs` / `tts_llm.rs`
     / `TtsEpisodeSummary`: an LLM writes a *text script*, held in an
     in-memory-only `Arc<Mutex<Vec<TtsEpisodeSummary>>>` that
     `ffi/register.rs` rebuilds empty on every `register` (so generated
     episodes are lost on restart); on `play` it dispatches a live
     `VoiceCommand::Speak` (AVSpeech). No audio file, no library episode, no
     persistence. **No iOS View/sheet dispatches its `generate`/`play`
     actions** — every Swift `tts`/`TtsEpisode` reference is the Agent
     subsystem or a Bridge type-mirror; the `tts.rs` doc-comment claims about
     "the iOS sheet's Stepper" / "the iOS list renders it" describe UI that
     does not exist.

  The reconciliation was a **human-decision gate** (AGENTS.md fragmentation, D7,
  the feature-#41 briefings precedent), not net-new persistence code. The
  options weighed were:
  - **Option A — adopt the Swift composer, delete the kernel stub. (CHOSEN.)**
    Point #43 at the agent-tool path; remove the orphaned kernel `podcast.tts`
    handler + `TtsEpisodeSummary` snapshot leg + `tts_llm.rs`. Lowest-risk;
    matched the only path with any UI/audio/persistence today. Executed in
    `feat/m9-delete-tts-stub`.
  - **Option B — make the kernel path real by dispatching synthesis to Swift.**
    (Not taken.) Would add a capability routing the kernel `generate`/`play`
    actions to `AgentTTSComposer` (kernel stays SSOT, Swift owns audio). More
    plumbing; only worth it if the kernel TTS surface were meant to grow its own
    UI — it is not.
  - Rust-native audio synthesis was rejected as a fix: it reintroduces the
    binary-transport blocker (iOS `HttpCapability` body is UTF-8 String only —
    see the M8-Blossom note — so synthesized audio bytes cannot transit
    Rust↔Swift) and duplicates what `AgentTTSComposer` already does natively.

  Remaining follow-ups (now tracked on the surviving Swift path, not this
  deleted stub): NIP-F4 publishing of agent episodes, deletion cleanup of
  `agent-episodes/<id>.m4a`, and verifying the published `.synthetic` episode
  metadata round-trips the store's disk layer across restart.
- **ai-chapters-real-generation.** Replace equal-length stub chapters with
  transcript/LLM-grounded chapters, provenance, regeneration/clear behavior,
  and persistence.
- **m4-chapters-rust-persistence.** Rust round-trip DONE — the original
  premise (chapters mutate Swift state only, no Rust action to receive them)
  was superseded by PR #175, which moved chapter synthesis into the kernel:
  `ai_chapters` calls `store.set_episode_chapters`, which writes `ep.chapters`
  (serialized to `podcasts.json` — the field is
  `serde(skip_serializing_if = "Option::is_none")`, not skipped) and flushes to
  disk, and `ffi/snapshot.rs` already projects chapters (incl. `is_ai_generated`
  + `source`) from the store onto `EpisodeSummary`. The remaining live gap —
  AI chapters flashing empty on a feed-refresh — was that `merge_episodes`
  (`host_op_handler_helpers.rs`) only carried `position_secs` forward, so a
  re-parsed RSS episode (chapters=None) clobbered them in memory before
  `subscribe()` re-persisted. Fixed: `merge_episodes` now carries prior
  AI-generated chapters forward when the fresh episode supplies none (publisher
  chapters still win — D7). Remaining follow-up (separate, iOS-only): the now-
  redundant chapters fallback in `AppStateStore+KernelProjection.swift`
  (`if episodes[idx].chapters?.isEmpty != false { ... = prior.chapters }`) can
  be deleted to finish the preserved-state-block removal.
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
- **android-download-capability-wiring.** `capabilities/DownloadCapability.kt`
  (OkHttp pull-model executor) ships compiling and unit-validated on the Rust
  side, but is **not yet instantiated** in `MainActivity`: `reconcile()` is
  never called from the snapshot poll loop and `detach()` is never called
  before `bridge.free()`, so episode enclosures do not actually download on
  Android yet. Follow-up: `remember` the capability alongside
  `ExoPlayerCapability`, drive `reconcile(snapshot.downloads?.active)` from the
  `LaunchedEffect` poll tick, and call `detach()` in `onDispose` ahead of
  `bridge.stop()/free()`. Also revisit the WorkManager-vs-foreground-scope
  trade-off documented in `DownloadCapability.kt` for background completion.
- **android-auth-keychain.** Replace Android `signinNsec` stub with a real
  secure-storage identity sheet mirroring iOS.

## Active P2 - Cross-Cutting Technical Debt

- **m5-non-utf8-feed-bodies.** Widen HTTP capability body transfer to preserve
  non-UTF8 feed bytes. Update Swift and Rust so XML encoding declarations are
  honored.
- **m8-blossom-body-base64-rust-side.** The iOS HTTP capability now decodes a
  `body_base64` request field to raw `Data` before sending it as the HTTP body
  (`App/Sources/Capabilities/HttpCapability.swift`), so binary uploads survive
  the UTF-8 bridge. The Rust side on `feat/m8-blossom-upload` does **not** use
  it yet: `apps/nmp-app-podcast/src/blossom.rs` still puts the base64 string in
  the existing `body` field, and `apps/podcast-feeds/src/http.rs`'s
  `HttpRequest` has no `body_base64` field. Until both are updated to emit
  `body_base64`, the Blossom upload silently sends base64 *text* as the HTTP
  body and is **not** end-to-end functional. Follow-up: add `body_base64` to the
  Rust `HttpRequest` struct and have `blossom.rs` set it instead of `body`.
- **m5-chirp-headers-parity.** Reconcile podcast-player and Chirp HTTP header
  schemas once the canonical `nmp-core::capability::http` shape lands.
- ~~**m8-blossom-binary-body.**~~ Done (Rust side): `HttpRequest` now carries
  binary bodies in a dedicated `body_base64` field
  (`apps/podcast-feeds/src/http.rs`), and the Blossom upload
  (`apps/nmp-app-podcast/src/blossom.rs`) emits the base64 blob in
  `body_base64` with `body: None` instead of stuffing base64 *text* into the
  UTF-8 `body` field. The iOS executor decodes `body_base64` back to raw
  `Data` before sending and prefers it over `body`
  (`App/Sources/Capabilities/HttpCapability.swift`, PR #174), so binary audio
  uploads survive the bridge intact and the path is end-to-end functional once
  the Swift change merges.
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
- **m1.6-kernel-widget-position.** Once `AudioCapability.sendReport` is wired
  to the Rust kernel (M1.6), kernel-projection position ticks will drive
  `nowPlaying.positionSecs`. At that point `PlatformCapability.applyNowPlayingSnapshot`
  needs a separate position-write path (not gated by the identity dedup) so the
  widget stays live during playback. Owner: M1.6 agent.

## Pending Decisions

- **podcast-keys-keychain-m7 (follow-up to M6-part-B).** Rust now persists
  per-podcast NIP-F4 secrets to plaintext `<data_dir>/podcast-keys.json`
  (`PodcastKeyStore::set_data_dir`/`save`, schema_version 1, wired through
  `nmp_app_podcast_set_data_dir`) and the Swift `PodcastKeysKeychainMigration`
  copies that file JSON→Keychain on startup (account id
  `pcst.podcast.<podcast_id>.nipf4`, written via `PcstIdentityCapability.direct`).
  The Keychain copy is still **write-only**: Rust reads `podcast-keys.json` as
  the source of truth, and nothing reads the Keychain item yet. M7 must:
  (1) flip the Rust read path to recall secrets from the Keychain — but note
  `pcst.identity.capability` is **not reachable from Rust** today
  (`SyncCapabilityBridge` routes only http/audio/download), so this depends on
  **PD-019** (the keyring Rust→Swift contract) being built first, OR on a
  Swift-side read shim; (2) delete the `podcast-keys.json` write path once reads
  are Keychain-backed. **Cross-language contract:** the account-id format lives
  only in Swift (`PodcastKeysKeychainMigration.accountID(_:)`) — the Rust read
  path must reconstruct `pcst.podcast.<id>.nipf4` to match; there is no shared
  constant enforcing this. The JSON envelope contract (`schema_version` + `keys:
  [{podcast_id, secret_hex}]`) is now pinned on the Rust side by the
  `persisted_file_matches_swift_wire_contract` unit test.
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

- **voice-conversation-off-thread-dispatch-uaf.** Done on branch
  `fix/voice-conversation-uaf`. The original suggestion (route `Speak` back
  through the actor thread) was unreachable: pinned nmp-ffi rev `ec15ede`
  exposes no accessor to clone the capability-callback slot and no seam to
  post a closure onto the actor thread, and the dep must not be forked.
  Instead `VoiceConversationManager` now retains the outer turn `JoinHandle`s
  and exposes `shutdown()` (abort + `block_on(join)`); `nmp_app_podcast_unregister`
  calls it — contractually before `nmp_app_free` — so every in-flight `app`
  dereference is fenced before the allocation frees. A `Drop` impl could not
  serve as the fence: the snapshot-projection closure holds a second strong
  `Arc<PodcastHandle>`, so the manager drops during `nmp_app_free` (after the
  actor join), too late. A `shutting_down` flag makes any late
  `on_transcript_final` a no-op.
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
