# Backlog

This is the tactical queue for active work, follow-ups, and pending decisions.
Do not duplicate these items in `WIP.md`; `WIP.md` only records branches and
worktrees currently in flight.

- **open-search-nostr-result-await (#605).** NMP #597 has landed and iOS
  `AddByURLForm` now uses the framework `nmp_app_intent_classify` /
  `nmp_nip21_decode_uri` path for Nostr profile/address inputs, with
  `nmp_app_intent_dispatch` used to start NIP-05 resolution and Add Show
  awaiting the async `resolved_profiles` projection with a bounded timeout
  before subscribing. Add Friend now uses the same NMP dispatch/projection path
  before adding a resolved NIP-05 pubkey as a friend. The legacy
  `podcast.open_search` compatibility scaffold has been retired so native
  text-entry surfaces use the NMP intent ABI directly. Remaining work: wire any
  future Android text-entry subscribe surface through the same NMP intent ABI.
  `AddFriendSheet.swift` and TUI
  `handle_subscribe_input` use the NMP intent ABI for Nostr refs while
  preserving their existing fallback paths. `NostrDiscoverForm.swift` now
  dispatches query searches through the NMP open-search path and renders
  relay-targeted NIP-50 results separately from already-fetched NIP-F4
  discovery rows. Owner: whoever continues #605.

- **knowledge-ann-index.** `top_k_search` is O(N) linear scan over all embedded chunks (fine for < ~50k chunks). When the corpus exceeds ~50k chunks, replace with an ANN index (e.g. HNSW via `usearch` or `instant-distance`). Slot in `podcast-knowledge::search::top_k_search` call site in `knowledge_search.rs`. <!-- TODO: ANN index when corpus > ~50k chunks -->

- **android-delete-wire-type (#573, pending).** Android unsubscribe affordance now routes to `UnfollowPayload` (issue #603, keep-history path). A permanent hard-delete affordance ("Delete") needs its own wire type (`DeletePayload` dispatched on `PodcastNamespace.PODCAST`) once the Rust `podcast.unsubscribe` → `podcast.delete` rename is complete (see `rust-unsubscribe-action-rename`). No hard-delete payload in Android until that rename lands.

- **rust-unsubscribe-action-rename (#573, pre-existing).** The Rust `podcast.unsubscribe` action (`PodcastAction::Unsubscribe`) performs a full hard-delete (removes the podcast row + episodes), which is a legacy misnomer now that `podcast.unfollow` is the keep-history path and all user-facing "Unsubscribe" maps to unfollow. Rename `podcast.unsubscribe` → `podcast.delete` across Rust (action enum + dispatch), iOS (`kernelUnsubscribe`/`deletePodcast` wiring), and Android wire types so the action name matches its hard-delete semantics. Pure rename, no behavior change; kept out of #547 to bound that PR's scope.

## Active P0 - Correctness Before More Features

- ~~**p0-nipf4-wire-contract.**~~ Done in PR #89: aligned kind `10154`/`54`
  builders and parsers with the NIP-F4 wire contract; removed non-NIP-F4
  `d`/`a`/`published_at`/`imeta` tags; round-trip tests verify absence.
- ~~**p0-nipf4-real-keys.**~~ Done: file-backed persistence to `podcast-keys.json` (atomic write/rename), reload on restart, key cleanup on `remove_owned_podcast`. Keychain migration deferred indefinitely — file storage is the canonical path.
- ~~**p0-nipf4-sign-and-publish.**~~ Done: `sign_event` produces real secp256k1-signed events with valid `id`/`pubkey`/`sig`; publish paths use the configured write-relay list and return `"published"` on relay acceptance. `relay_pending` status removed.
- ~~**p0-nipf4-relay-discovery.**~~ Done: kind:10154 show discovery
  uses `NostrDiscoveryObserver` + `EnsureInterest` through the NMP relay pool,
  and feedless NIP-F4 subscription dispatches `SubscribeNostr`, opens a kind:54
  author-filtered interest through `push_interest_via_nmp`, then upserts
  inbound episodes via `NostrEpisodesObserver`. HTTP gateway search remains a
  convenience path; the Nostr-only subscribe path no longer depends on RSS.
- ~~**p0-nipf4-author-claim.**~~ Done: `publish_author_claim` signs with active agent key and publishes kind:10064. Called after create/update/delete of owned podcasts.
- **p0-plan-truthfulness.** Keep `docs/plan.md`,
  `docs/plan/nmp-feature-parity.md`, and this backlog synchronized with code.
  Do not mark scaffolded behavior done. Current audit: `docs/plan.md` now
  reflects the `Cargo.toml` NMP v1.0.0-rc.1 pin at rev
  `1fc3e6bea390224cef30e37d2ccaa90615197521`, plus known drift against the
  Chirp-shipped NMP pin `bc6b42592d7fd61bc6767cac246a24a6b23bf8e3`. Open
  tracking remains #707 for re-pin/publish lifecycle/relay-config persistence,
  #708 for pollable NIP-05 lookup state, #709 for action/projection codegen
  drift checks, and #734 for D8 sleep/polling paths found by the NMP scanner.
  `docs/testing/chirp-nmp-validation-pack-2026-07-06.md` and the
  `05-chirp-nmp-regression-parity.md` catalog file map those gaps into 56
  scenario pages.
  Current report-generator follow-up: the per-scenario Pages contract now
  front-loads launch readiness, risk class, evidence quality, evidence
  placeholders, whole-product coherence, and generated GitHub issue-backed
  validation blockers before any page can appear launch-ready. Scaffolded
  blocker links do not count as observed run evidence; real scenario evidence
  still has to attach screenshots, UI trees, metrics, accessibility audits,
  logs, cassettes, and revalidation IDs.
  Remaining parity debt lives in `App/Sources/` Swift policy/fallback code plus
  the listed platform/AI gaps. Agent chat title generation now routes prompt
  construction, message filtering/truncation, selected model, and JSON title
  parsing through Rust (`nmp_app_podcast_agent_chat_title_*`); Swift only
  executes the provider call and persists the returned title. Nostr peer-agent
  reply framing now routes peer-channel prompt construction, npub encoding, and
  owner-vs-peer fallback wording through Rust (`nmp_app_podcast_agent_nostr_peer_prompt`);
  Swift passes raw profile/owner facts and executes the agent turn loop. Main
  in-app agent system prompt construction now routes through Rust
  (`nmp_app_podcast_agent_system_prompt`): Swift supplies raw agent context,
  friends, notes, memory facts, and skill catalog rows while Rust owns prompt
  prose, section ordering, caps, truncation, and fallback wording. Conversation
  history tools now route list/search policy through Rust
  (`nmp_app_podcast_agent_conversation_history`): Swift supplies raw in-app and
  Nostr transcript facts while Rust owns source normalization, caps, ordering,
  lexical matching, snippet truncation, Nostr display fallbacks, and tool row
  shape. The agent voice-list tool now routes query matching, caps, row shaping,
  and result counters through Rust (`nmp_app_podcast_agent_voice_list`) after
  the existing Rust-backed ElevenLabs catalog fetch; Swift only passes raw voice
  catalog rows and tool args. The YouTube search tool now routes argument
  normalization, limit caps, and final result shaping through Rust
  (`nmp_app_podcast_agent_youtube_search_*`); Swift only executes the extractor
  search capability with the Rust-planned query/limit. The podcast-directory
  search tool now routes query/type normalization, limit caps, date formatting,
  and final result shaping through Rust
  (`nmp_app_podcast_agent_directory_search_*`); Swift only executes the
  directory capability and passes raw hit facts back. The agent category-list
  tool now routes include-podcasts parsing, caps, generated-at formatting, row
  shaping, and counters through Rust (`nmp_app_podcast_agent_category_list`);
  Swift only passes raw category summaries from the Rust-owned library category
  projection. Simple inventory list tools (`list_podcasts`, `list_subscriptions`,
  `list_in_progress`, `list_recent_unplayed`) now route caps, timestamp
  formatting, envelopes, and row shaping through Rust
  (`nmp_app_podcast_agent_inventory_list`); Swift only passes raw rows from the
  Rust-owned inventory adapter.
- **p0-validation-gate.** Established for current merge gates: branch protection
  requires deterministic merge contexts for `Git diff hygiene`, `Migration
  lint gates`, `Rust workspace build gate (all members, all targets)`, `Swift
  bridge codegen drift gate`, `Android Kotlin compile + unit tests`, `Android
  cross-compile check (aarch64-linux-android)`, and `Headless e2e kernel
  proofs (nipf4 signing + offline scenarios)`, plus the full iOS simulator
  `Build and Test` lane. `Build and Test` runs
  `ci_scripts/run_tests.sh`, which builds the Rust core for
  `aarch64-apple-ios-sim` and runs `xcodebuild ... test` without
  `SKIP_UI_TESTS` in the regular Test workflow. The route back to making it
  required started after PR #495's run
  `27500102726` failed on the main-equivalent merge commit with the test host
  restarting during `AppTests.testPositionUpdatesAreDebounced`, a failing
  `AppStateStorePerformanceTests.testUnplayedCountIsConstantTime`, and a
  cascade of app-process-loss UI failures in `CoreJourneyUITests`,
  `P0PlaybackUITests`, `P1SettingsUITests`, and `SetupSubscribeUITests`.
  PR #496 isolates unit-test stores from automatic episode metadata indexing,
  adds `xcodebuild` retry-on-failure coverage to `ci_scripts/run_tests.sh`,
  and skips UI screenshot/tree attachments once the app is no longer
  foreground. Local validation on 2026-06-14 passed
  `SKIP_UI_TESTS=1 ./ci_scripts/run_tests.sh` (669 tests, 3 skipped, 0
  failures) plus focused simulator tests for
  `AppStateStorePerformanceTests` and
  `AppTests.testPositionUpdatesAreDebounced`; the full UI lane still failed
  locally in `CoreJourneyUITests.testP0_03_PlayStartsAudio` and
  `CoreJourneyUITests.testP0_04_ResumeReopenByTitle`, so full `Build and
  Test` should remain non-required until the post-#497 full-lane evidence is
  clean. PR #497 fixed the local playback UI blockers by seeding downloads into
  the canonical download store, isolating UI-test lifecycle teardown, making
  Now Playing artwork safe for MediaPlayer's off-main renderer, and hardening
  the playback reopen plus launch-metric flows. The focused local validations
  passed for `UITestSeederTests/testSeededDownloadURLUsesCanonicalDownloadStorePath`,
  `CoreJourneyUITests/testP0_03_PlayStartsAudio`,
  `CoreJourneyUITests/testP0_04_ResumeReopenByTitle`, and
  `SmokeUITests/testColdLaunchPerformance`.
  The post-#497 main Test workflow exposed a separate Android CI harness bug:
  Kotlin compile and unit tests succeeded, then `gradle/actions/setup-gradle@v3`
  failed during cache cleanup against Gradle 8.7 with a write-only
  `removeUnusedEntriesOlderThan` property. PR #499 upgrades the action to
  `gradle/actions/setup-gradle@v6.2.0` and uses the current `cache-cleanup`
  input so the required Android gate reports the real build result, and removes
  obsolete `/tmp/nmp-at-ac7e307e` clone steps left over from the deleted NMP
  path patch. The same PR's non-required full iOS lane then timed out before
  tests while `tuist generate` was resolving Swift packages. The follow-on
  bootstrap fix pins the remaining Swift package ranges to their already
  resolved release revisions and wraps `tuist generate` in bounded retry/cleanup
  logic so a package-resolution stall cannot consume the entire Build and Test
  job. The main run at `104ff094` proved the watchdog path still needed one
  more hardening pass: `tuist generate` timed out at 600s, killed the stalled
  resolver, but the wrapper treated the killed attempt as success and let
  `run_tests.sh` continue. `xcodebuild test` then independently hung while
  fetching `LiteRT-LM` and the job was cancelled before tests. The follow-on
  fix makes watchdog timeouts return a failed attempt, retries Tuist only after
  cleaning generated state, and makes `run_tests.sh` perform its own bounded
  SwiftPM resolve before invoking `xcodebuild test` against the resolved
  package set/cache. The Test workflow's simulator lane also generates without
  the LiteRT-LM SwiftPM package by touching `.ci-disable-litertlm-package` before
  `tuist generate`, because
  LiteRT-LM's remote binary artifacts exceed the 30-minute simulator lane
  budget and its device-only local-model probes skip on simulator; default
  local/TestFlight generation still includes the real package. The clean
  main-equivalent evidence is Test workflow run `27509095557` on commit
  `bde6e7695066ea7e3ae3f37ad01ad44cc1778d90`: `Build and Test` completed
  successfully at `2026-06-14T19:43:14Z` after PR #504 landed, and branch
  protection now includes `Build and Test` in the required status-check
  contexts.
  The old `nmp-blossom` portability blocker is resolved on `main`, PR #498
  removed the temporary `vendor/nmp-core` fork, and the Rust/headless required
  merge gates are locally unblocked by the upstream-pinned NMP rev. Do not
  reintroduce app-local fake rev bumps, `publish_outbox` suppression, or
  ADR-0055 oracle disablement.
- ~~**p0-ios-test-target-compile.**~~ Fixed across PR #101 and PR #102:
  `Nip46RemoteSignerTests.swift` now accepts an optional bunker pubkey, the
  active Tuist target no longer references the dead `KernelModel` duplicate,
  and `App/Sources/AppIntents/PlaybackAppIntents.swift` uses a Notification
  bridge that compiles in the `Podcastr` target. Remaining shortcut hardening
  stays tracked under `appintents-validation`.
- ~~**p0-ios-test-target-compile-regression.**~~ The compile drift resolved
  as part of the NaN-frame-drop fix (PR for `fix/nan-frame-drop-and-ios-test-target`):
  verified `xcodebuild build-for-testing -only-testing:PodcastrTests` returns
  `TEST BUILD SUCCEEDED` on current `main` (e1b7b151); the five drifted test
  files (`LocalModelCatalogMatchTests`, `LocalLLMInferenceTests`,
  `LLMProviderTests`, `ClipBoundaryResolverTests`, `AgentContextDecodeTests`)
  now compile against current APIs. Remaining pre-existing assertion failures
  in `UserIdentityWiringTests` (Nostr tag assertions, social module) are
  unrelated to compilation and tracked separately.

## Active P1 - Compat And Ownership Burn-Down

- ~~**inbox-triage-on-async-subscribe.**~~ Done (PR #383 deferred; completed in
  this PR). `PodcastAppState.inbox` flipped to `Arc<InboxState>`; 6th ctor
  arg added to `FeedFetchCoordinator::new`; `apply_subscribe_result` calls
  `self.inbox.maybe_enqueue_triage()` gated identically to
  `auto_categorize`/`auto_refresh_picks`. Test:
  `subscribe_report_with_fresh_episodes_enqueues_triage` asserts
  `triage_in_progress` flips. Golden 3789 B byte-identical. Closes item.

- ~~**external-feed-ensure-kernel-seed.**~~ Done in this PR:
  `SubscriptionService.ensurePodcast` now dispatches the typed `podcast`
  action `{"op":"ensure_podcast"}`. Rust ingests the feed as a known podcast,
  persists episodes, projects `is_subscribed`/`last_refreshed_at`, and Swift only
  creates `PodcastSubscription` rows for summaries whose follow flag is true.
  The old Swift-only `FeedClient.fetch` → `store.upsertPodcast` /
  `store.upsertEpisodes` path has no production caller.
- ~~**threading-projection-kernel-ownership.**~~ Done in this PR:
  cross-episode threading topics and mentions now come from
  `nmp_app_podcast_threading_projection`, derived in Rust from kernel library,
  transcript, and categorization facts. The Swift `ThreadingInferenceService`
  no longer creates topics, writes mentions, seeds DEBUG mock rows, normalizes
  slugs, persists threading arrays in `AppState`, or decides the Home
  "Threaded Today" active-topic gate. `nmp_app_podcast_threading_active_topics`
  applies the unplayed/archive/category filter and returns the exact mention
  ids for the playlist; Swift only refreshes and renders the Rust projection.
- ~~**home-related-kernel-ownership.**~~ Done in this PR:
  Home's Related sheet now calls `nmp_app_podcast_knowledge_home_related`.
  Rust owns seed-query construction from title/TOC chapters, topic-vs-sources
  lens limits, seed filtering, one-row-per-show topic collapse, and the
  category fallback when the transcript index is empty. Swift only maps
  returned episode ids to native navigation rows.
- ~~**owned-podcast-episode-backfill-kernel.**~~ Done: kernel `update_owned`
  now detects a private→public flip and calls `publish_episode` for every
  episode atomically; the Swift loop deleted (PR #396).
- ~~**compat-service-stubs-delete.**~~ REMOVED — referenced path does not exist at origin/main.
- ~~**compat-domain-stubs-delete.**~~ REMOVED — referenced path does not exist at origin/main.
- ~~**compat-kernelmodel-delete.**~~ REMOVED — referenced path does not exist at origin/main.
- ~~**compat-useridentity-delete.**~~ REMOVED — referenced path does not exist at origin/main.
- ~~**identity-kernel-actions.**~~ Done in this PR. The active Swift and
  Android identity flows dispatch kernel-owned import nsec, generate, clear,
  publish profile, bunker connect/disconnect, nostrconnect, and cancel-handshake
  paths. `AccountSummary` now also carries the Rust-derived short fingerprint
  (`sha256:` + first 16 hex chars of SHA-256 over decoded pubkey bytes), and
  the iOS account details screen renders that projection instead of hashing in
  Swift.
- **settings-provider-ownership.** Move OpenRouter mode, BYOK-imported
  credentials metadata, provider settings, and onboarding gate decisions into
  Rust-owned settings projections/actions. Delete Keychain-only UI fallbacks
  once the kernel can represent the state. Provider HTTP transport is part of
  this ownership boundary: OpenRouter/Ollama chat/completion, embeddings,
  model catalog discovery, OpenRouter credential validation, OpenRouter
  Whisper/STT, ElevenLabs Scribe/STT multipart upload, and AssemblyAI
  submit/poll STT now live in the shared Rust backend, with iOS/Android/TUI
  only supplying credentials, selected models, typed audio source intent, and
  UI. Android now reloads encrypted OpenRouter/Ollama keys into Rust, exposes
  typed credential settings for those providers, and calls shared Rust
  OpenRouter key validation through generated UniFFI. Swift live
  wiki/title/categorization/chapter/clip completion callers no longer
  preflight OpenRouter/Ollama Keychain keys before invoking the shared Rust
  provider transport, and Swift OpenRouter settings validation no longer
  preflights Keychain before calling the shared validator. Swift Episode
  Diagnostics no longer hides the OpenRouter Whisper retry path behind a
  Keychain preflight; forced OpenRouter Whisper retries now call the shared
  Rust STT transport so missing-key/provider errors come from the backend.
  Swift provider completions no longer reject `.local` before dispatch; the
  Rust provider transport is the single owner for unsupported-provider and
  missing-credential semantics.
  Agent similar-episode search now calls a Rust `knowledge_similar_episode`
  FFI so the kernel owns seed episode lookup, query derivation, retrieval, and
  seed filtering instead of Swift building a title/description query locally.
  Search-tab local show/episode search now calls `nmp_app_podcast_local_search`;
  Rust owns followed-feed scope, archived-episode visibility, tokenization,
  scoring weights, snippet selection, ranking, and caps while Swift resolves
  returned ids for native row rendering.
  Swift Perplexity search no longer trims/rejects empty queries before
  dispatch; Rust online-search transport owns query normalization, invalid-query
  errors, provider fallback, and missing-credential semantics. The agent tool
  layer also stopped pre-checking `perplexity_search.query` and
  `find_similar_episodes.seed_episode_id`; those requests now flow to the
  Rust-backed transport/seed lookup.
  Swift image generation no longer infers missing-credential errors by scanning
  backend strings; Rust provider transport is the only source of image
  generation failure semantics.
  ElevenLabs key validation now also runs through the shared Rust backend
  (`/v1/user`), and ElevenLabs Scribe transcription now uses
  `nmp_app_podcast_elevenlabs_scribe_transcribe` so Rust owns selected Scribe
  model lookup, ElevenLabs auth, local-file/source_url multipart shaping,
  status handling, and response parsing. AssemblyAI transcription now uses
  `nmp_app_podcast_assemblyai_transcribe` so Rust owns the selected model
  fallback list, AssemblyAI auth, `/v2/transcript` submit/poll contract,
  status handling, response parsing, and usage telemetry normalization.
  Agent Perplexity search now uses `nmp_app_podcast_perplexity_search` so Rust
  owns direct Perplexity `/v1/sonar`, OpenRouter fallback, credential priority,
  status handling, and source parsing. iOS/Android/TUI mirror ElevenLabs,
  AssemblyAI, and Perplexity credentials into the same in-memory provider-key
  action as OpenRouter/Ollama. Android now mirrors ElevenLabs/STT provider
  settings, stores OpenRouter/Ollama/ElevenLabs/AssemblyAI/Perplexity keys in
  its encrypted host store, reloads them into Rust on app start, reports STT
  key presence to Rust, exposes shared agent chat, ElevenLabs validation,
  Scribe/AssemblyAI/online-search UniFFI calls, and exposes STT/TTS model
  settings plus an ElevenLabs voice browser through typed settings actions
  backed by the shared Rust voice catalog. Shared provider
  catalog rows now expose a routed `selection_model_id`, and iOS/Android/TUI
  selectors store that value so OpenRouter/Ollama selections run the intended
  provider/model. The TUI env loader now forwards `ASSEMBLYAI_API_KEY` and
  `PERPLEXITY_API_KEY` into the shared provider-key cache, and its ElevenLabs
  voice row now browses the shared Rust `/v1/voices` catalog instead of making
  users paste raw voice ids. Non-realtime ElevenLabs TTS synthesis for voice
  previews, pick rationale narration, and generated podcast speech turns now
  uses `nmp_app_podcast_elevenlabs_tts_synthesize`, so Rust owns TTS
  credentials, selected-model fallback, request shaping, provider status
  handling, and audio response normalization. Swift auto-ingest candidate
  selection and transcript readiness UI now consume the kernel-resolved
  effective STT provider instead of re-checking OpenRouter/ElevenLabs/
  AssemblyAI Keychain values locally, and stale Swift-side OpenRouter/Ollama
  RAG key probes plus online-search endpoint aliases have been removed. Rust
  settings snapshots now project non-secret
  provider key-presence booleans for OpenRouter/Ollama/ElevenLabs/AssemblyAI/
  Perplexity, Android mirrors those fields and uses them for credential-card and
  speech-provider readiness, and Swift provider/transcript/wiki readiness UI
  consumes the shared projection instead of Keychain-only status fallbacks.
  Rust settings actions/projections now also persist non-secret credential
  source, BYOK key id/label, and connected-at metadata for AssemblyAI and
  Perplexity, matching OpenRouter/Ollama/ElevenLabs; iOS/Android/TUI set,
  clear, and display that metadata instead of boolean-only provider status.
  Speech STT/TTS model options now come from the shared Rust
  `nmp_app_podcast_speech_model_catalog` instead of Swift/Android-owned
  constants, with the TUI using the same catalog for provider-setting display
  and input hints. On-device model metadata now comes from the shared Rust
  `nmp_app_podcast_local_model_catalog` instead of Swift-owned constants, with
  Swift/Android/TUI consuming the same ids, names, download URLs, sizes, and RAM
  floors. Ollama chat endpoint defaults and URL normalization now live in the
  shared Rust provider settings path; iOS/Android/TUI render the canonical
  projected URL and submit raw endpoint intent instead of applying platform-
  specific fallback rules. Swift's live JSON completion wrapper and model
  catalog/browser types are now provider-neutral (`ProviderCompletionClient`,
  `ProviderModelCatalogService`, `ProviderModelSelectorView`) to keep OpenRouter
  as one shared Rust-routed provider rather than a Swift-owned backend concept.
  The unused Swift `LLMProviderCredentialResolver` policy shim was removed so
  credential requirement decisions cannot drift back into platform code.
  BYOK provider authorization now uses shared Rust entry points for
  provider-scope mapping, PKCE/state generation, callback validation, and
  `/api/token` exchange; Swift and Android only present native browser/callback
  UI and persist returned secrets in their secure host stores. Shared model
  routing now treats blank/`none` credential sources as disconnected, so
  platform credential clears cannot accidentally route bare model IDs through
  OpenRouter. Android's ElevenLabs voice picker now consumes the shared Rust
  voice catalog. AssemblyAI/Perplexity credential metadata now matches
  OpenRouter/Ollama/ElevenLabs across Rust settings, projections, iOS, Android,
  and the TUI. Remaining provider-ownership work is streaming voice-mode
  STT/TTS once the canonical NMP capability seam lands upstream
  (`pablof7z/nostr-multi-platform#954`).
- **typed-agent-task-intents.** Backend `AgentTaskIntent` creation exists and
  the TUI task editor now submits typed/natural task requests instead of raw
  dispatch namespace/body JSON. Agent task snapshots now expose user-facing
  intent metadata and hide raw dispatch namespace/body fields from serialized
  projection JSON. Android task creation now uses `create_from_intent` with an
  variant-backed `AgentTaskIntent` payload. The parked `ios/` shell is deleted,
  and the active Swift scheduled prompt tool/settings surface now creates and
  edits `agent_prompt` tasks through
  `podcast.tasks` typed intents, renders the shared `agentTasks` projection,
  lets Rust parse/reject cadence strings and missing task IDs, and dispatches
  Rust-owned `run_due` on foreground instead of scanning a persisted Swift task
  array. Agent task rows now persist through the shared
  Rust sidecar so disabled, edited, deleted, and completed tasks survive kernel
  restarts across iOS, Android, and TUI. Keep raw `create` as
  compatibility/internal only; remaining work is a durable background-agent
  execution/history model for prompt tasks if they should remain isolated from
  the main agent chat.
- **relay-list-ownership.** App relay configuration now has one owner:
  the NMP `configured_relays` projection plus `add_relay`/`remove_relay`/
  `set_relay_role` ops on `podcast.settings`. The old Swift/Rust
  `nostrPublicRelays` / `nostr_public_relays` settings mirror and Agent
  Podcasts relay editor have been removed; legacy saved keys are ignored and
  dropped on the next persistence write. Remaining NIP-65 work is the real
  user/agent kind:10002 publish/refresh model, not another app-side relay
  array. Nostr discovery no longer gates rendering on the legacy Swift
  `settings.nostrRelayURL`; the view claims the Rust discovery interest and
  Rust/NMP owns relay/indexer routing.
- ~~**relay-config-c-abi-persistence.**~~ DONE (commit `0dcf9680`, PR #220
  "persist relay configuration across app restarts via C-ABI path"). Relay
  edits now survive restarts via a `.nmp-relay-config.json` sidecar — the same
  on-disk shape the template builder uses (one canonical file). Load happens in
  `ffi/data_dir.rs:112` (`store::relay_config::load_relay_config`), called
  from `nmp_app_podcast_set_data_dir`; save happens in
  `host_op_handler/settings_actions.rs:391` → `ffi/relay_persist.rs`
  (`persist_configured_relays`) after each relay mutation. The default-relay
  seed in `register.rs` remains unconditional (the slot is empty at register
  time because the actor hasn't run `Start` yet), but persisted edits now
  correctly override it on subsequent launches.
- ~~**app-relays-config-ui.**~~ Done (`feat/app-relays-ui`). The App Relays editor
  ships at Settings → Networking → App Relays: `AppRelaysView` lists
  `configuredRelays` (color-coded role pill, swipe-to-delete → `kernelRemoveRelay`,
  tap-row → `ChangeRelayRoleSheet` role picker, empty state), `AddRelaySheet`
  (URL + role picker, `wss://`/`ws://` validation → `kernelAddRelay`), and a
  shared `AppRelayRole` model keyed to the kernel's canonical role strings
  (`read` | `write` | `both` | `indexer` | `both,indexer`). `NetworkingSettingsView`
  now NavigationLinks to the editor with a relay count and relabels the legacy
  single relay as "Agent Relay". Consumed the Rust prerequisite from
  `feat/podcast-relay-ops` (PR #202): `configured_relays` projection +
  `add_relay`/`remove_relay`/`set_relay_role` on `podcast.settings`. Restart
  durability of edits is shipped (see `relay-config-c-abi-persistence`, now DONE).
- ~~**snapshot-push-delivery.**~~ Done across the per-domain typed sidecar and
  report-channel follow-ups: the active Swift bridge consumes NMP update-sink
  pushes through `PodcastHandle.listen`, uses `SnapshotUpdateSignal` /
  `ActorCommand::MarkChangedSinceEmit` to wake autonomous background changes,
  and keeps volatile playback/download ticks on narrow report channels plus
  content-hash gates instead of the old fixed 500 ms snapshot poll. Remaining
  terminal-client snapshot-revision polling is tracked separately in
  `docs/plan/shared-llm-task-architecture.md`.
- ~~**capability-router-unify.**~~ Done in this PR: `SyncCapabilityBridge` is
  now only the non-MainActor C callback adapter. `PodcastCapabilities` owns the
  namespace routing contract, including the async HTTP namespace, and the bridge
  uses the same canonical `HttpCapability` instance instead of constructing a
  second executor/report path. HTTP remains actor-thread safe; main-actor
  capabilities route through the shared table on the main thread.

## Active P1 - Tier 1 Usability Hardening

- **tui-feature-parity-followups.** `apps/podcast-tui` now has a parity
  foundation for bookmarks, clips, agent, wiki, social, settings, and queue
  surfaces (see `docs/plan/tui-parity.md`). The agent slice now wires chat,
  picks, task CRUD/run, memory CRUD, and agent-note fetch/publish where the
  kernel has real actions. The downloads slice now wires active queue rows,
  progress/detail rendering, pause/resume/cancel/cancel-all, delete-file
  routing, and per-episode active/completed badges. The episode-detail slice
  now renders transcript/chapter/summary/comment/ad-segment projections and
  dispatches fetch transcript, fetch/compile chapters, summarize, fetch/post
  comments, reset progress, and sleep timer actions. The settings relay slice
  now wires configured relay add/remove/role editing and validates projection
  updates. The provider/model settings slice now wires LLM role model
  selection, provider credential metadata for OpenRouter/Ollama/ElevenLabs/
  AssemblyAI/Perplexity, env-backed in-memory provider credentials,
  STT key-presence reporting, STT/TTS model selectors, shared ElevenLabs voice catalog
  selection, and the local model hint. The terminal shell now
  has shared animated chrome, focused row rails, player waveform motion,
  download activity strips, and themed detail/input overlays without changing
  kernel/provider behavior. Post-architecture live tmux validation now covers
  GLM chat, memory, typed tasks, providers, RSS, queue/bookmark/playback,
  relaunch persistence, no-identity notes, and clean fallback playback. The
  final GLM 5.1 Cloud pass after the provider-neutral and Rust scheduler merges
  repeated those core scenarios with no additional TUI fixes required.
  Remaining terminal-client slices: settings editors for playback intervals,
  notifications, onboarding, and Nostr profile/public relay fields; wiki
  generation/search and richer agent note trust/conversation workflows once the
  corresponding kernel behavior is real; centralized completed-download history
  when the kernel projects it; and focused TUI integration scenarios beyond the
  current subscribe/queue/settings/agent/download/detail/relay smoke.
- ~~**bunker-isconnecting-reactive.**~~ Done in this PR.
  `RemoteSignerView` no longer owns a local `isConnecting` flag that clears
  immediately after the fire-and-forget `signInBunker` dispatch. Remote-signer
  pairing state now lives in `UserIdentityStore`, stays pending across nil or
  stale local-key snapshot ticks, clears when a remote `activeAccount` appears,
  and fails with a timeout if no terminal kernel state arrives.
- **rss-subscribe-validation.** Swift subscribe entry points share one
  feed-URL normalizer: unsupported schemes are rejected before kernel dispatch,
  missing schemes are canonicalized to `https://`, and duplicate followed
  feeds are detected against that canonical URL even when the user omits the
  scheme. Android search-result subscribes now use the same validation shape:
  invalid/non-HTTP feeds are not dispatched and bare hosts are canonicalized
  before `SubscribePayload`. Remaining: provider errors, restart persistence,
  and empty/error UI.
- **opml-import-export-hardening.** OPML import now rejects oversized files and
  unbounded feed counts, reports invalid/non-HTTP feed URLs as partial row
  failures instead of silently dropping them, and keeps valid rows importable.
  OPML export now skips feed-less podcasts and de-dupes repeated feed URLs so
  round-tripped exports do not reintroduce duplicates. iOS still previews parsed
  rows locally for the import sheet, but all subscription writes continue
  through `kernelSubscribe`; the Rust `import_opml` action uses the same
  bounded parser/reporting path for non-UI callers.
- **feed-refresh-hardening.** Validate cold start, foreground refresh,
  conditional GET, failure reporting, notification hooks, and auto-download
  hooks.
- **player-device-validation.** Validate play/pause/seek/speed/sleep/end item,
  lock-screen metadata, remote commands, route changes, AirPlay, and background
  behavior on simulator and device.
- **queue-hardening.** Validate item-ended advancement, duplicate handling,
  remove/clear, persistence expectations, and UI sync.
- ~~**remote-command-kernel-routing.**~~ Done in this PR. Lock-screen /
  Control Center Play still enters through `AudioCapability.execute(.play)`,
  but the `PlaybackState` command handler now stages the restored episode in
  Rust with `kernelLoad` before starting the engine whenever the kernel
  snapshot has no `nowPlaying.episodeId`. Rust-originated plays already have a
  staged episode, so the guard is a no-op and cannot loop through the echoed
  `.load` command. Regression coverage lives in
  `PlaybackRemoteCommandRoutingTests`.
- **download-state-projection.** Runtime queue projection is now wired:
  player download actions mutate `DownloadQueue`, download reports update
  progress/paused/failed/completed state, and snapshots expose active/queued/
  paused/failed rows instead of only completed local paths. Cold-start full
  pulls now accept the first equal-rev snapshot so a partial push cannot hide
  the disk-restored library, and projection coverage proves reloaded completed
  downloads still surface `download_path`/size. Delete failures now keep the
  episode projected as downloaded and emit diagnostics until a later delete
  succeeds or the file is already gone. Remaining: validate background
  URLSession restore and offline-first playback on device.
- **settings-completion.** Finish playback/settings projection parity:
  skip intervals, auto-skip ads, streaming/offline preferences, onboarding
  gate, provider settings, and persistence migration.
- **notification-hardening.** Validate authorization, schedule/update/cancel,
  deep links, duplicate prevention, and quiet failure behavior.
- ~~**stale-subscription-refresh-test.**~~ Done in this PR. The stale Swift
  `SubscriptionRefreshServiceTests` coverage was replaced with Rust-owned
  refresh result tests for the live path: parsed `200` results update podcast
  metadata and episodes, while `304 NotModified` results persist refreshed
  validators without touching episodes or bumping the snapshot revision.

## Active P1 - Social/Nostr Real Logic

- ~~**perpodcast-publish-followups.**~~ Done. PR #444/#446 added the live
  headless register→sign proof for `publish_show` and `publish_episode`: the
  scenario drives a real kernel signer registration, signs kind:10154/54 events
  with the per-podcast pubkey, and proves the active account does not switch.
  PR #448 changed `delete_owned` to emit a single NIP-09 kind:5 request with
  both `["k","10154"]` and `["k","54"]`, so deleting an owned podcast tombstones
  both the show event and all per-podcast episode events.
- ~~**social-bunker-signing-kernel.**~~ DONE (D13, PR to be merged). Both
  `.localKey` and `.remoteSigner` (NIP-46 bunker) identities publish through
  `podcast.social` → kernel `publish_unsigned_event` → `sign_active_nonblocking`
  → `PendingSign` park for remote signers. There was never a Swift NIP-46 signing
  branch in `UserIdentityStore+Publishing.swift`; the stale comment that claimed
  one was removed in this PR. Verified against NMP v0.6.2 rev ac7e307e
  `crates/nmp-core/src/actor/commands/publish.rs` + `pending_sign.rs` +
  `nip46_bunker_signing.rs` integration test.
- **nostr-publish-confirmation-projection.** `publishProfile` / `publishUserNote`
  are now honest fire-and-forget (`Void` return, no fabricated signed-event stub).
  The UI shows "Profile update sent." rather than "Profile published." because relay
  confirmation never surfaces synchronously. To show a true "published" state, add a
  snapshot projection in the kernel that emits the signed event `id`/`sig` after NMP
  confirms relay acceptance, surface it via `AppStateStore` snapshot update, and wire
  `EditProfileView` to observe the projection field (keyed by pubkey + kind:0) before
  flipping to a "Published" banner. Deferred because NMP does not currently project
  post-publish event identity back to the host app.
- ~~**social-profile-name-about-completion (#601).**~~ Done in this PR:
  Swift and Android decode `name`/`about` from the kernel-projected
  `AccountSummary`, profile edit screens hydrate from `active_account`, and the
  iOS UserDefaults plus Android SharedPreferences profile-field mirrors were
  removed. Rust `IdentityStore` remains the single local source for accepted
  kind:0 profile fields.
- **social-notes-friends-kernel-wiring (#601).** Foundation Rust stores
  (`store/notes.rs`, `store/friends.rs`) are in place. Local notes now have
  `SocialAction` mutations plus `PodcastUpdate.notes`/`podcast.misc.notes`
  Rust projections decoded by iOS and Android. iOS local note add/update/delete/
  restore/clear call sites now dispatch Rust note actions and reconcile
  `AppState.notes` from `PodcastUpdate.notes`, including episode/friend/note
  anchors. Rust-owned friends now have `AddFriend`/`UpdateFriendName`/
  `RemoveFriend` actions, `PodcastUpdate.friends` plus `podcast.social.friends`
  projections, Android DTO/domain-frame decode, and iOS friend mutations plus
  rendering state reconcile through `AppState.friends`. iOS now captures the
  legacy Swift `AppState.notes` / `AppState.friends` payload at launch and seeds
  Rust `notes.json` / `friends.json` once after the kernel data dir is bound,
  with the `socialNativeStoreMigrationV1` flag set only after all dispatches are
  accepted. Android was audited for the same migration seam and keeps no
  SharedPreferences-backed notes/friends mirror, so there is no Android data to
  backfill. Remaining:
  - Move any remaining Android note/friend UI rendering seams to Rust-owned
    lists instead of platform-local mirrors.
- **local-notes-kernel-store.** Publishing user notes is already Rust-owned via
  `podcast.social.publish_note`, and local note persistence/projection/actions
  now exist in the kernel. The native shells still render and mutate Swift
  `Note` rows through `AppState.notes`, but iOS note mutations now dispatch the
  Rust local-note actions, every kernel snapshot replaces `AppState.notes`
  from `PodcastUpdate.notes`, and the one-time iOS native-store migration seeds
  already persisted Swift notes into `notes.json`. Remaining: Android note UI
  adoption if/when it grows local note surfaces, and eventual deletion of the
  Swift note persistence field once migration is complete. Keep publish routing
  through `podcast.social` and do not reintroduce Swift signing/tag policy.
- ~~**nip73-formatting-kernel.**~~ Done. The legacy Swift `publishUserClip`
  helper has been retired; clip publish/update semantics now remain tracked
  under `autosnip-real-boundaries` so they can be owned from the Rust clip
  lifecycle instead of a parallel Swift helper. Rust `build_highlight_tags`
  owns the NIP-73/84 `r`, `i`, `context`, and `alt` tag assembly, with typed
  action and pure tag-builder tests locking the contract.
- ~~**social-publish-relay-target.**~~ Done. User-signed social publishing
  routes through `nmp_dispatch.rs` with `PublishRaw { target: Auto }`, and the
  pinned NMP `Nip65OutboxResolver` resolves `Auto` through cached author write
  relays plus the active account's locally configured write relays during the
  no-kind:10002 bootstrap window. The stale `relay.primal.net` comment no
  longer exists on current `main`.
- ~~**episode-comments-relay-wiring.**~~ DONE (verified at `apps/nmp-app-podcast/src/comments_handler.rs`).
  Real kind-1111 relay subscribe/publish is wired: `handle_fetch_comments` 
  (line 61) subscribes via `push_interest_via_nmp` with kind:1111 + NIP-73 `#i` tag filter;
  `handle_post_comment` (line 103) publishes via `publish_raw_via_nmp` (line 140);
  `CommentsObserver` (line 164) receives inbound events and caches by episode.
  Episodes are mapped to anchors via `episode_nip73_anchor` (line 70, 129).
- ~~**social-graph-store-wiring.**~~ ~~Replace `social_handler.rs` `nostr_pending`
  with NMP kind:3 contact-list store reads, kind:0 metadata hydration,
  subscription refresh, and snapshot updates.~~ Closed — replaced by reactive
  `FollowListProjection` + `ActiveFollowSet` (nmp-nip02) in PR
  `feat/social-graph-reactive-trust-gate`. `handle_fetch_contacts` is now a
  lightweight refresh trigger; the NIP-02 follow list populates via the kernel's
  standing `account_profile_interest` subscription (no relay pull). Trust gate
  for `AgentNoteSummary::trusted` wired via `ActiveFollowSet::predicate()`.
- **nostr-conversations-real-projection.** Replace compat-empty
  conversation/approval surfaces with Rust-owned conversation projection,
  trust-list/approval actions, kind:0 profile cache, and NIP-46
  integration.
  - **DONE — kernel approve/block allowlist.** `ApprovedPeerStore`
    (BTreeSet JSON, atomic tmp+rename, D6 load) wired into
    `SocialState::trust_predicate` (`(followed || approved) && !blocked`),
    `agent_note_handler` responder gate, `data_dir.rs` cold-load, and
    `social_actions.rs` host-op handler. iOS dispatch shims added
    (`KernelModel+Social.swift`); `SocialSnapshot` now projects explicit
    approved/blocked pubkey arrays from Rust so `AgentAccessControlView`
    renders Rust-owned lists instead of Swift mirror sets. Dead
    `NostrPendingApproval` / `NostrApprovalPresenter` scaffolding deleted.
    Follow-ups: bridge-decode fixture test for `trusted`/approved/blocked
    fields; Android access-control UI.
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
  - **DONE — trust gate (projection-time-live).** `AgentNoteSummary::trusted`
    is computed at **projection-build time** in
    `SocialState::agent_notes_snapshot` by applying the shared live
    `ActiveFollowSet::predicate()` to each cached note's author hex (PR
    `feat/social-graph-reactive-trust-gate`). The verdict is NEVER frozen at
    receipt: a note from X received before following X starts untrusted and
    flips to `trusted: true` on the next projection after the follow lands (and
    back on unfollow). Per-account social state (`social_slot` + `agent_notes`)
    is cleared on account switch so no cross-account trust/notes leak.
    Conversations projection deferred to `nostr-conversations-real-projection`
    (next cycle).
  - ~~**DONE — LLM responder loop (PR #421).**~~ `agent_note_responder.rs` +
    `agent_note_responder_tests.rs` implement the inbound→LLM→outbound autopilot
    in the kernel: dedup via `ResponderCache`, per-root turn cap, `wtd-end`
    end-conversation gate, bounded kind:0 profile hydration, owner-consult `ask`
    tool. Wired at `agent_note_handler.rs:332` (`with_responder`). The Swift
    `NostrAgentResponder` deletion is permanent; all auto-reply logic now lives
    in Rust (D7/D13).
  - Non-goal: NIP-17 (private direct messages) is out of scope for agent
    coordination and will not be used for this purpose.

## Active P1 - AI Scaffold Replacement

- **episode-pipeline-followups.** Deferrals from the kernel-owned episode
  pipeline event-log + auto-download work (`feat/episode-pipeline-events`):
  1. ~~**auto-download mode collapse.**~~ DONE (PR #503). The kernel now stores
     typed `AutoDownloadMode { Off / LatestN(n) / AllNew }` — no longer a flat bool.
     Evidence: `apps/nmp-app-podcast/src/store/auto_download.rs:54` (enum def),
     `apps/nmp-app-podcast/src/ffi/snapshot_library.rs:122-128` (projection),
     `App/Sources/Bridge/AppStateStore+KernelActions.swift:410-427` (dispatch),
     `App/Sources/Bridge/AppStateStore+KernelProjection.swift:281-294` (decode).
  2. ~~**ad detection not in the kernel.**~~ DONE (PR refactor/kernel-ai-chapters-ad-spans).
     `ai_chapters_llm.rs` now emits ad spans; `ai_chapters.rs` persists via
     `set_ad_segments_for` and emits `ads.ready`. `AIChapterCompiler.swift` deleted.
  3. ~~**AI chapters not reported to the kernel from the legacy path.**~~ DONE (same PR).
     All call sites dispatch `podcast.chapters.compile`; the Swift writer is removed.
- ~~**inbox-triage-progress-projection.**~~ DONE in this PR. The
  `inbox_triage_in_progress` bool is projected onto `PodcastUpdate` and
  `HomeFeaturedSection.isStreaming` is wired to it. Rust now also projects
  `inbox_last_triaged_at: Option<i64>` from the latest Ready triage-cache entry
  through the full snapshot and `podcast.library` domain sidecar; Swift decodes
  and passes it into `HomeFeaturedSection.lastTriagedAt`, and Android mirrors
  the same full/domain field.
- **agent-chat-real-loop.** Replace canned assistant responses with real LLM
  streaming, tool execution, progress/cancel states, memory/context policy,
  provider errors, and transcripted tool results.
- **rag-vector-search-real.** Replace substring search with
  `podcast-knowledge` indexing, embeddings, BM25/KNN retrieval, scoped
  search, provenance, and reindex jobs.
- **coreml-embeddings-activation (#236).** The on-device Core ML MiniLM
  embedding path is fully wired but INACTIVE: `CoreMLEmbeddingProvider`
  (384-dim, `#if os(iOS)`/`@available(iOS 16,*)`) + bundled WordPiece tokenizer
  (`bert-vocab.txt`) + `LocalEmbeddingsClient` (cloud-fallback adapter) ship in
  PR for #236 and are composed into `RAGService`, but `LocalEmbeddingsClient`
  routes to the cloud until BOTH gates clear. Remaining to activate:
  (1) **Publish the `.mlpackage` asset** — run `coremltools` to convert
  `sentence-transformers/all-MiniLM-L6-v2`, host it, and add a `LocalModelSpec`
  for `all-minilm-l6-v2` to the Rust catalog (`apps/nmp-app-podcast/src/llm/
  local_model_catalog.rs`); note that catalog's test currently pins
  `.litertlm`/`huggingface.co`, and `DownloadCapability.localModelFileURL`
  forces `.litertlm` while embedding models use the new `.mlpackage` helper —
  the unified download executor's `.localModel` destination must learn the
  embedding-model extension. (2) **Index dimension migration** — the live
  `VectorIndex` is 1024-dim (`text-embedding-3-large`); MiniLM is 384-dim, so
  `prefersLocal` stays false on the existing index. Activating on-device
  embeddings requires opening the index at 384 and re-embedding the corpus (a
  reindex job), or a side-by-side index. `LocalEmbeddingsClient` deliberately
  refuses to mix dimensions, so wiring is safe to ship inert. (3) Surface the
  active/downloading/ready state in the AI settings UI (the readiness plumbing
  exists via `LocalEmbeddingsClient.prefersLocal` + `EmbeddingProvider.isReady`).
- **wiki-real-generation.** Replace placeholder wiki articles with RAG-backed
  synthesis, citations, refresh/invalidation, per-podcast storage, and delete
  semantics.
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
  2. **No iOS voice-mode provider adapter exists.** The
     `Capabilities/Tts/{ElevenLabsAdapter, AvSpeechAdapter}` referenced in the
     `voice.rs` doc comment are unwritten; iOS voice mode is
     `SFSpeechRecognizer` (STT) + `AVSpeechSynthesizer` (TTS) only. Episode
     transcription now has shared Rust OpenRouter Whisper, ElevenLabs Scribe,
     and AssemblyAI transports, but those are not voice-mode realtime STT/TTS
     adapters. A provider-routed TTS path also needs an audio sink (the
     synthesized bytes must reach `nmp.audio.capability` or the OS audio
     engine).
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
     present). Episode STT execution paths have since landed for Scribe,
     AssemblyAI, and OpenRouter Whisper; revisit whether a cloud default is
     appropriate for key-configured users separately from voice-mode routing.
  Suggested landing order once unblocked: (1) add `provider` + provider-scoped
  voice to the wire schema; (2) iOS ElevenLabs TTS adapter + audio sink behind
  that provider; (3) route `eleven_labs_voice_id`/`eleven_labs_tts_model` in
  `VoiceConversationManager` (D7: Rust decides voice) only when the active
  provider can honor it; (4) provider-routed voice-mode STT/TTS execution path
  + reconcile the cloud `stt_provider` default for key-configured users (the
  keyless `apple_native` default already landed in PR #178); (5) design
  barge-in-threshold + OpenRouter-TTS
  settings, then wire barge-in and OpenRouter TTS.
- **voice-mode-elevenlabs-tts-playback-sink.** The kernel-driven voice executor
  (`VoiceCapability.speak`) now *routes* on the projected `eleven_labs_voice_id`
  (set ⇒ user chose ElevenLabs TTS) but falls back to AVSpeech with an honest
  log line because there is no audio playback sink in this path:
  `ElevenLabsTTSClient.synthesizeStream` yields raw audio `Data` frames and the
  only consumer (`AudioConversationManager.beginSpeaking`) records them for
  barge-in and marks playback "future work" — no `AVAudioPlayerNode` route is
  wired through `AudioCapability`. Non-realtime ElevenLabs TTS now uses the
  shared Rust backend; this item is only about realtime voice-mode streaming
  playback. To make ElevenLabs TTS audible in the
  kernel-driven path: add a player-node sink (likely via `AudioCapability`),
  feed `ElevenLabsTTSClient` frames into it, and emit `started`/`finished`
  `VoiceReport`s from real playback callbacks. Until then the fallback is the
  correct behavior. Note: this is separate from the parallel SwiftUI
  `AudioConversationManager` voice path used by `VoiceView`, which has the same
  missing-sink gap.
- **agent-ask-tool-kernel-ownership.** `AgentAskCoordinator` currently owns the
  owner-consultation tool contract in Swift: FIFO prompt queueing, the
  five-minute timeout, and sentinel tool outputs (`"user declined to answer"`,
  `"user did not respond within 5 minutes"`). Rust now owns ask normalization,
  FIFO/current promotion, timeout duration, decline/timeout sentinel strings,
  timeout wakeup, and the final agent tool result envelope through `agent_ask`
  FFI. Swift renders the current prompt, parks continuations, reports raw
  answer/decline outcomes, and resumes from the Rust ask callback when timeout
  expiry settles a pending ask. Remaining: pending asks still use a focused FFI
  callback rather than a general Rust-pushed pending-ask projection/action
  lifecycle; migrate to a projection if/when other shells need to render this
  queue.
- ~~**apple-directory-search-kernel-ownership.**~~ DONE. The agent
  `search_podcast_directory`, directory collection-id lookup, and Add Show
  Apple Podcasts search/trending paths no longer construct Apple URLs, issue
  `URLSession` requests, parse iTunes JSON, rank top-chart lookup rows, or
  filter malformed feed URLs in Swift. Swift sends raw user intent to
  `nmp_app_podcast_itunes_directory_search`,
  `nmp_app_podcast_itunes_lookup_feed_url`, and
  `nmp_app_podcast_itunes_top_podcasts`; Rust owns endpoint shape, storefront
  top-chart lookup, limit clamping, HTTP capability dispatch, podcast-vs-episode
  row parsing, publish timestamps, duration conversion, feed-backed row
  filtering, ranking preservation, and error envelopes. Swift only decodes the
  Rust-authored envelope into existing agent/UI value types.
- **agent-episode-mutation-kernel-ownership.** Agent tools for
  `mark_episode_played`, `mark_episode_unplayed`, and `download_episode` now
  dispatch raw episode ids first and surface Rust rejection for unknown
  episodes; the general Swift download wrapper also no longer resolves or
  passes enclosure URLs. Rust now also builds the agent-facing
  `EpisodeMutationResult` envelope via `episode_mutation_tool_result`, including
  episode title, podcast id/title, state, and unknown-id rejection; Swift only
  dispatches the mutation and relays the Rust-authored result for these tools.
  Agent inventory tools now also call `nmp_app_podcast_agent_inventory`; Rust
  owns subscribed/all-podcast scope, Unknown-podcast suppression,
  unplayed/archive counts, in-progress/recent-unplayed filters, ordering, caps,
  and per-podcast episode listing, while Swift only decodes rows into the
  existing tool protocol value types.
- **agent-playback-kernel-ownership.** `play_episode` for library episodes now
  routes playback intent through Rust `podcast.player` actions, including
  bounded segments (`start_seconds` / `end_seconds`) and queue placement.
  Rust owns whole-episode `play`, canonical id lookup, resume position,
  unknown-id rejection, download-on-play, and validated whole-episode enqueue /
  enqueue-next. Rust `play` / `load` actions now accept
  optional `start_secs` / `end_secs`, stage `PlayerState.segment_end_secs`, and
  stop or Rust-auto-advance when `AudioReport::Playing` reaches the segment end.
  Rust queue entries can now carry transient bounded-segment starts/ends, and
  auto-advance / explicit play-next stages those bounds through the Rust player.
  `LivePlaybackHostAdapter` now dispatches play/enqueue intent to Rust first and
  reads the kernel `PlayerState` projection for agent-visible now-playing,
  seek, and skip responses. Agent playback controls for pause, speed, sleep
  timer, seek, and skip now dispatch through Rust player actions; omitted skip
  intervals are resolved from Rust-owned settings. External URL `play_episode` and
  `generate_tts_episode(play_now: true)` also start through
  `podcast.player.play` instead of constructing local Swift-only episodes.
  Rust queue entries now persist bounded segment starts/ends, project them on
  queue rows, and the Swift Up Next shell syncs from that Rust projection
  instead of stripping queued segments to whole-episode IDs on restart.
  Rust queue rows now carry stable Rust-owned slot ids and Swift remove/reorder
  affordances dispatch those slot ids back to `podcast.player` instead of
  mutating only local state. The old Swift `currentSegmentEndTime` bounded
  segment auto-advance path has been retired; segment boundaries are decided by
  Rust audio reports. Library `play_episode` and `get_now_playing` agent result
  metadata now come from Rust `playback_tool_result` /
  `now_playing_tool_result` instead of Swift-projected episode/podcast rows.
  Agent playback-rate requests now pass raw speed to Rust and report the
  Rust-clamped applied rate. Agent `seek_to` now passes raw target seconds to
  Rust; the player actor clamps to `[0, duration]` when duration is known,
  updates `PlayerState.position_secs`, and Swift reports that Rust-applied
  position. In-app, CarPlay, and lock-screen playback-rate changes now dispatch
  raw speed to `podcast.player.set_speed`; Rust clamps/stages
  `PlayerState.speed` and the native shell only executes the returned
  `AudioCommand::SetSpeed`. External URL `play_episode` now asks Rust for an
  `external_play_plan` before creating the placeholder podcast/episode and
  returns Rust `playback_tool_result` metadata instead of deriving the
  parent/result envelope in Swift. Remaining: Swift still executes the
  background feed metadata hydration for external-play placeholders as a host
  capability; keep that host-side unless Rust grows a feed-hydration job
  lifecycle projection/action.
- **agent-transcription-kernel-ownership.** `request_transcription` now asks
  Rust to accept the `queued` transcript status and surfaces Rust rejection for
  unknown episodes before starting the native ingest service.
  `download_and_transcribe` now follows the same pattern for queued status and
  Rust-owned download dispatch. The Swift agent tool layer no longer performs
  local episode-existence preflights. Transcript ingest now asks Rust for a
  ready/skipped/publisher/STT plan; Rust owns publisher-first, AI fallback,
  provider/key gating, per-show opt-out, Apple-native local-file gating, and
  auto-ingest eligibility while Swift only executes the returned host
  capability branch and reports results. Episode Diagnostics now renders the
  Rust ingest plan instead of mirroring the readiness decision tree in Swift.
  Agent transcript tool result formatting now comes from Rust
  `transcript_tool_result` instead of Swift switching over `TranscriptState`.
  The unused Swift `TranscriptionQueue` was deleted because it still encoded a
  publisher-vs-Scribe fallback policy despite having no live callers.
  Remaining: `download_and_transcribe` still awaits native ingest execution in
  Swift. Keep that as host capability execution unless the kernel grows a
  transcript-job lifecycle projection/action that can own completion waits.
- **tts-episodes-reconcile-two-mechanisms (feature #43) — RESOLVED.**
  **Option A chosen — kernel stub deleted, Swift `AgentTTSComposer` is
  canonical.** The orphaned kernel `podcast.tts` vertical (`tts.rs`,
  `tts_llm.rs`, `TtsEpisodeModule`/`TtsEpisodeAction`, the `TtsEpisodeSummary`
  projection + `PodcastUpdate.tts_episodes` snapshot leg, the in-memory
  `tts_episodes` slot, and their tests) was removed in `feat/m9-delete-tts-stub`.
  The Swift agent-tool path (`generate_tts_episode` → `AgentTTSComposer`) is now
  the single TTS mechanism. Generated episodes are inserted into the Rust
  kernel store with `podcast.add_episode`, and play-now dispatches
  `podcast.player.play` instead of driving `PlaybackState` directly. Rust-only change: the Swift Bridge mirror
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

  The reconciliation was a **human-decision gate** (AGENTS.md fragmentation, D7),
  not net-new persistence code. The
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

  Generated-episode metadata planning is now Rust-owned via
  `nmp_app_podcast_agent_tts_episode_plan`: Swift supplies raw execution facts
  (turn text, measured durations, source episode title/artwork) and the kernel
  derives chapter grouping, fallback labels, timed transcript segments, flat
  transcript text, and inherited artwork before Swift dispatches the existing
  `podcast.add_episode` action. Agent default voice fallback is also Rust-owned
  via `nmp_app_podcast_agent_tts_default_voice`, and `configure_agent_voice`
  dispatches the existing `set_eleven_labs_voice` settings action through
  Rust instead of mutating a Swift `Settings` mirror. The per-episode NIP-F4
  publish gate is Rust-owned too: `publish_episode` now rejects missing,
  private, disabled, or unowned episodes in the kernel, while Swift only
  dispatches the intent. The default feed-less generated-show descriptor
  (stable id, title, description, author, visibility, categories) is also
  supplied by Rust via `nmp_app_podcast_agent_generated_podcast_descriptor`.
  Owned-podcast metadata edits and visibility toggles now dispatch
  `update_owned_podcast` for both public and private transitions and wait for
  Rust projection instead of mutating a Swift podcast render mirror; owned
  deletion dispatches only `delete_owned_podcast`, not a second Swift
  unsubscribe. The dead Swift `upsertPodcast` / `updatePodcast` mutation
  helpers were removed so durable podcast row writes have no local bypass.
  Remaining follow-ups (now tracked on the
  surviving Swift path, not this deleted stub): NIP-F4 publishing of agent
  episodes, deletion cleanup of `agent-episodes/<id>.m4a`, and verifying the
  published `.synthetic` episode metadata round-trips the store's disk layer
  across restart.

  Projection gap resolved: generated episodes and unfollowed external RSS
  ensure now ride the Rust projection. Keep this TTS item scoped to surviving
  Swift composer follow-ups
  (NIP-F4 publishing, deletion cleanup, restart verification), not feed-store
  ownership. Agent default voice selection now uses the Rust-owned
  `eleven_labs_voice_id` / `eleven_labs_voice_name` provider settings path
  instead of a private Swift `UserDefaults` key; `configure_agent_voice`
  dispatches the existing `set_eleven_labs_voice` settings action through
  `store.updateSettings`, and generated speech turns read the projected kernel
  voice when no per-turn `voice_id` override is supplied.
- ~~**ai-chapters-swift-compiler-delete.**~~ DONE (PR #refactor/kernel-ai-chapters-ad-spans).
  `AIChapterCompiler.swift` deleted; call sites in `PlayerView`, `EpisodeDetailView`,
  and `TranscriptIngestService` converted to `kernelCompileChapters`; FULL + ENRICH-ONLY
  modes + ad validation ported to Rust. The `KernelProjection` legacy fallback also removed.
- ~~**m4-chapters-preserved-state-cleanup.**~~ DONE (same PR). The legacy Swift-chapters
  fallback block in `AppStateStore+KernelProjection.swift` has been deleted.
- **inbox-triage-real-model.** Replace recency heuristic with provider-backed
  triage, persisted dismiss/listened state, explainable reasons, and user
  correction loop. Partially done in PR #123 (rig-core + Ollama LLM scoring
  wired; remaining items below).
- ~~**inbox-triage-async-streaming.**~~ DONE (PR #173 / M5.1). All LLM triage
  work runs off the actor thread via `runtime.spawn` → `tokio::task::spawn_blocking`.
  Evidence: `apps/nmp-app-podcast/src/inbox_handler.rs:43-44` (module doc),
  `apps/nmp-app-podcast/src/inbox_handler_triage.rs` (spawn paths at lines 130
  and 238-239). The actor is never blocked; each batch bumps the rev counter
  incrementally as results land.
- ~~**inbox-triage-cache-persist.**~~ DONE (PR #244). `inbox_triage_cache`
  (`HashMap<String, TriageResult>`) is persisted to
  `<data_dir>/inbox-triage-cache.json` (JSON, atomic write). Evidence:
  `apps/nmp-app-podcast/src/store/inbox_triage_cache.rs:1-38` (full module:
  load/save over `Path`, D6 silent-degrade). Cold launches reload prior scores;
  stale `Pending` entries retry via normal cooldown.
- **agent-tasks-real-scheduler.** Rust now parses task schedules, projects
  `next_run_at`, supports `run_due`, and owns Swift foreground catch-up policy
  for shared task rows. Rust now persists `agent_tasks` across kernel restarts.
  Remaining: add notification/OS wake integration, durable retry policy, and a
  background-agent execution/history model for arbitrary prompt tasks.
- **agent-picks-controls-validation.** Rust now owns personalized picks
  ranking (`picks_handler.rs` + `picks_llm.rs`) and Home renders
  `PodcastUpdate.picks` through `HomeRecommendedSection`. Remaining: explicit
  opt-out/reset controls, refresh UX validation, and tests that the old Swift
  curation service cannot re-enter the user-visible path.
- **categorization-ownership-split.** Rust now owns AI episode tags and
  category aggregates (`EpisodeSummary.aiCategories` +
  `PodcastUpdate.categories`), but Settings/Home category management still uses
  the separate Swift `PodcastCategorizationService` / `AppStateStore+Categories`
  model keyed by `subscriptionIDs`. Decide whether those Swift categories are a
  distinct user-curated section model (rename/document accordingly) or migrate
  category generation, corrections, persistence, and localization into the Rust
  categorization projection/actions.
- **autosnip-real-boundaries.** Rust now owns autosnip capture/refinement for
  the `podcast.clip.auto_snip` path: shells dispatch the playhead/source, the
  kernel creates a pending clip, refines from timed transcript entries when
  present, and re-refines pending clips when `transcript_report` later supplies
  timing. Manual composer saves and agent-created clips now dispatch
  `podcast.clip.create` with a caller-provided UUID while Rust snaps manual
  ranges to timed transcript entries, derives clip text/speaker there, and
  can fall back to explicitly supplied text only when timed entries are not
  available. iOS agent/manual callers no longer send tool- or Swift-derived
  transcript text into creation; they read Rust-projected clip text after the
  mutation.
  Quote-share boundary resolution now dispatches `podcast.clip.resolve_quote`
  and renders only the kernel-returned transcript-aligned segment; Swift no
  longer computes a local transcript-segment fallback when the kernel cannot
  resolve quote bounds.
  The legacy Swift `AppState.clips` mirror, local `addClip` helpers, preview
  seeding, and clip-specific Swift identity publishing helper/tests have been
  retired so the kernel projection is the only app clip list source.
  Rust persists `ClipRecord` rows in `clips.json` and hydrates them during
  `nmp_app_podcast_set_data_dir`, so clips survive restart without a Swift
  state mirror. The Rust clip handler now records `clip.created` diagnostics
  with span/source details, replacing the old Swift `addClip` event side
  effect. User-visible non-agent clips are published as kind:9802 highlights
  from the Rust clip lifecycle after create, or after transcript-report
  refinement when an autosnip was initially pending. The app-local identity
  import/generate/load path now also registers the same key as NMP's active
  signer, so Rust-owned clip publishing does not depend on Swift publish
  helpers to self-heal identity state. The clip composer now opens the real
  share/export sheet from the Rust-projected clip after dispatching the create
  action instead of sharing from a local draft placeholder. Clip audio/video
  export now consumes the Rust-projected downloaded file path on
  `Episode.downloadState` rather than recomputing download ownership through
  `EpisodeDownloadStore`. The iOS sleep timer now dispatches Rust-owned
  playback actions for duration and end-of-episode modes; native Swift only
  holds the OS countdown timer and reports expiry back to the kernel.
  The `summarize_episode` agent tool now dispatches the raw episode id through
  Rust and surfaces the kernel rejection instead of running a separate Swift
  `episodeExists` preflight.
  Remaining: real video export generator-track
  implementation (native AVFoundation capability work, not clip state/policy),
  and a Rust result/projection contract for agent-created clips so Swift no
  longer has to read episode metadata only to fill `ClipResult`. Agent clip
  creation now dispatches raw episode ids/ranges and surfaces Rust rejection
  for unknown episodes or invalid ranges.
- **agent-memory-integration.** Finish moving agent memory out of Swift
  `AgentMemory` state and into Rust `MemoryFact`s. Live split: the Swift
  `record_memory` tool now writes Rust `MemoryFact`s through
  `nmp_app_podcast_memory_remember_text`, with Rust minting the canonical fact
  key and Swift activity undo dispatching `podcast.memory.forget` for that key.
  `AgentPrompt` now renders the Rust `PodcastUpdate.memoryFacts` projection
  instead of Swift `compiledMemory` / active memories, and the turn loop no
  longer runs the Swift `AgentMemoryCompiler`. `AgentMemoriesView` and settings
  counts now render/edit/delete Rust `MemoryFact`s through `podcast.memory`
  actions instead of UUID Swift rows. The Swift `AppState.agentMemories`,
  `compiledMemory`, `AgentMemoryCompiler`, and memory mutator extension have
  been deleted; data export now carries Rust-projected `MemoryFact`s explicitly,
  and clear-all dispatches `podcast.memory.forget_all`. Legacy
  `AgentActivityKind.memoryRecorded` remains only so pre-migration activity
  entries decode/display, but undo is inert because the old Swift memory store
  no longer exists. Complete migration still requires source attribution/privacy
  controls and any desired one-time import of old on-disk Swift memories into
  Rust facts.

## Active P1 - Platform And Android

- ~~**platform-widget-snapshot-codegen.**~~ DONE (PR #508). `WidgetSnapshot`
  and `HandoffState` are now under swift-codegen: generated to
  `App/Sources/Bridge/Generated/PodcastPlatformTypes.generated.swift` from
  `apps/nmp-app-podcast/src/bin/swift_codegen/emit.rs:1376-1419`. Hand-mirrored
  copies deleted. The CI drift gate covers this file.
- **carplay-validation.** Validate templates, now-playing sync, entitlement
  behavior, cold-connect placeholder, and playback dispatch on CarPlay
  simulator/head unit.
- **appintents-validation.** Validate Siri/Spotlight phrases, unavailable
  playback state behavior, localized phrases, and background execution. The
  active App target no longer uses the Notification bridge for playback
  shortcuts: Pause and Skip Forward dispatch `podcast.player` actions directly,
  and Resume dispatches `podcast.siri.resume` so Rust owns fallback selection.
  Reintroduce Play Latest only after the active app can route it through
  `podcast.siri.play_latest` instead of selecting episodes in Swift.
- **spotlight-hardening.** Validate indexing throttles, deletion/update, and
  no reindex churn from playback-position ticks. Podcast and episode rows now
  come only from `SpotlightCapability` over the Rust `PodcastSummary`
  projection; the legacy Swift `SpotlightIndexer` is note-only and clears its
  old subscription/episode domains instead of choosing followed shows or a
  latest-unplayed episode subset in Swift. Spotlight taps first decode the
  Rust-backed identifiers, with legacy note/subscription/episode identifiers
  retained only for stale OS rows.
- **handoff-hardening.** Validate NSUserActivity donation/invalidation,
  continue path, and stale activity behavior across devices.
- **icloud-settings-hardening.** Confirm Rust owns settings policy, conflicts,
  echo suppression, availability, opt-in behavior, and migration.
- **android-tier1-parity.** Finish Android parity gaps that still require
  user-visible work or policy validation: lock-screen MediaSession commands
  routed through Rust playback policy, Android keypair generation, and Tier 2+
  AI/Nostr/platform surfaces. Tier 1 library, search, subscribe, feed refresh,
  playback, sleep timer, playback queue, downloads, settings, BYOK nsec import,
  HTTP capability execution, and audio report round-trips now use the NMP
  kernel/capability path. Android can call the shared Rust provider
  complete/embed/catalog/image/rerank, online search, and cloud-STT transports
  through generated UniFFI, and model-role settings now load the shared Rust catalog for
  selection. Android also has encrypted OpenRouter/Ollama/ElevenLabs/
  AssemblyAI/Perplexity credential settings and reloads those keys into the
  Rust in-memory provider cache, including shared OpenRouter/ElevenLabs
  validation; remaining provider work is voice-mode provider
  execution/credential surface.
- ~~**android-gradle-wrapper.**~~ Done — `gradlew`, `gradlew.bat`, and the
  wrapper files are present under `android/Podcast/`; `./gradlew assembleDebug`
  is the validated Android build path.
- ~~**android-download-capability-wiring.**~~ Done — `MainActivity` owns the
  Android `DownloadCapability`, reconciles it from `snapshot.downloads.active`,
  and detaches it before the bridge is freed. Downloads intentionally stay on
  the pull-model executor so the Rust queue remains the single policy owner.
- ~~**android-auth-keychain.**~~ Done — PR #196. Remaining: key generation
  (kernel doesn't expose generated nsec to host yet).
- ~~**android-download-capability-anr.**~~ Done — `detach()` no longer blocks
  the main thread; it marks the capability detached, cancels tracked OkHttp
  `Call`s, cancels jobs, and suppresses late reports after bridge teardown.
- **android-exoplayer-position-sampling.** DOCUMENTED PLATFORM EXCEPTION
  (#322). Significant playback-position events on Android are reported
  event-driven via `Player.Listener` (`onIsPlayingChanged`,
  `onPlaybackStateChanged(STATE_ENDED)`, `onPositionDiscontinuity` for seeks,
  `onPlayerError`). Within-segment progress between those events still requires
  sampling: ExoPlayer (`media3` 1.4.1) exposes no per-second position callback
  and `getCurrentPosition()` is poll-only. `ExoPlayerReportListener` therefore
  keeps a `Handler` tick (`POSITION_TICK_MS`) that runs ONLY while playing and
  stops the instant `isPlayingChanged(false)` fires (no idle wakeups). Reduced
  from 250 ms (4 Hz) to 1000 ms (1 Hz), matching the iOS executor cadence and
  staying well under the canonical ≤4 Hz `AudioReport::Playing` ceiling. This
  is the platform constraint, not a polling hack; revisit only if a future
  media3 release adds a position-progress callback.
- ~~**tui-mpv-position-sampling.**~~ DONE (PR #507). The follow-up FFI wiring is
  landed. `poll_audio_position` in `apps/podcast-tui/src/runtime.rs:116-151`
  drains `AudioReport`s from `AudioHost` and forwards each one through
  `nmp_app_podcast_audio_report`. `AudioHost::poll_position`
  (`apps/podcast-tui/src/audio_host.rs:336`) pushes `AudioReport::Playing` with
  the real mpv position into `pending_reports`; `drain_reports`
  (`audio_host.rs:78-82`) flushes them to the runtime. The original platform
  exception (≤4 Hz IPC sampling) is unchanged and documented — what was missing
  was the kernel-report call, now present.

## Active P2 - Cross-Cutting Technical Debt

- ~~**swift-codegen-settings-snapshot.**~~ Done: `PodcastSettingsSnapshot.generated.swift`
  is now generator-owned — the last hand-maintained Generated/ file is gone.
  `SettingsSnapshot` needs a mixed `CodingKeys` enum (most keys auto-camelCase, ~15
  override to raw snake_case `ollama_chat_url`/`stt_provider`/`assembly_ai_*`, plus the
  BYOK ID/label uppercase-acronym fields). The field manifest gained an optional
  `coding_key_override: &'static str` (`Field::with_key`); `emit_settings_snapshot()`
  emits the struct, the full explicit `CodingKeys` enum, and the
  `decodeIfPresent`-with-defaults `init(from:)` (seeded from `self.init()`, so an absent
  key keeps its default and the decoder can never throw `keyNotFound`). Faithfulness
  proof: generator output is byte-for-byte identical to the prior hand-maintained body
  (only the header comment changed), so `git diff --exit-code App/Sources/Bridge/Generated`
  stays clean and the existing `.convertFromSnakeCase` decode-parity tests
  (`EffectiveSTTProviderDecodeTests`, `SettingsSnapshotParityTests`) pass unchanged.
  `main.rs` skip dropped; the CI drift gate now covers every Generated/ file.

- ~~**signed-events-fb-bridge.**~~ Done (PR #383): `nmp_app_podcast_decode_update_frame`
  now decodes the typed `signed_events` FlatBuffer sidecar
  (`nmp_core::decode_snapshot_typed_projections` + `nmp_core::typed_projections::decode_signed_events`)
  and injects the result under `v.projections["signed_events"]` so
  `SignedEventsRegistry.ingest` works unchanged. Absent/malformed sidecar degrades
  silently (D6). Swift unchanged. Regression since #377 (v0.3.0 typed-first migration).

- ~~**provider-api-keys-no-kernel-handler.**~~ Stale audit entry. Live Rust has
  `SettingsAction::SetProviderApiKeys` and
  `settings_actions.rs` stores the in-memory OpenRouter/Ollama secrets via
  `PodcastStore::set_provider_api_keys`. Broader provider ownership remains
  tracked under `settings-provider-ownership`.

- **observable-granularity-podcasts-subscriptions.** PR for
  `fix/observable-granularity` promoted `episodes` out of the single
  `AppStateStore.state` into its own `@Observable` stored property (the hot
  field that churns at playback / mark-played / triage cadence), so episode
  mutations no longer re-render settings/nostr/agent surfaces. `podcasts` and
  `subscriptions` were intentionally left inside `state`: they are cold-path
  (subscribe/unsubscribe only) and pulling them would have meant editing ~24
  more read sites for marginal benefit. Follow-up: if profiling shows library
  grid re-renders driven by a `state` write to a cold field, split
  `podcasts`/`subscriptions` out the same way (`store.podcasts` /
  `store.subscriptions`), re-composing them at the persistence seam alongside
  `episodes` in `runStateSideEffects` / `composedState`.
- ~~**m5-non-utf8-feed-bodies.**~~ Done in `codex/non-utf8-feed-bodies`: the
  HTTP capability now carries raw response bytes via additive `body_base64`,
  Swift/TUI/headless executors preserve those bytes, and Rust feed parsing
  prefers them so XML encoding declarations are honored.
- ~~**m8-blossom-body-base64-rust-side.**~~ Done (superseded by the
  `m8-blossom-binary-body` entry below). The Rust side now emits the blob in the
  dedicated `body_base64` field (`apps/nmp-app-podcast/src/blossom.rs`,
  `apps/podcast-feeds/src/http.rs`) and the iOS executor decodes it back to raw
  `Data`, so the Rust audio-upload path is end-to-end functional — the "Rust does
  not use it yet" status this item described is no longer true.
- ~~**blossom-active-account-upload-kernel.**~~ **DONE (PR feat/blossom-upload-via-nmp).**
  The avatar (`ChangePhotoSheet`) and artwork (`LiveAgentOwnedPodcastManager.generateAndUploadArtwork`)
  callers now dispatch `nmp.blossom.upload` through `KernelModel.blossomUpload` and
  await the `BlobDescriptor` from the drain-once `action_results` typed sidecar.
  `BlossomUploader.swift` is deleted. The Swift bridge no longer races this
  kernel-settled action against a local timeout; timeout/failure semantics stay
  with the Rust/NMP action owner. The `nmp-blossom` action module (v0.6.0) owns
  the full Build → Sign → Transport pipeline (D13/D0).
- **blossom-audio-path-migration.** Migrate the audio upload path
  (`apps/nmp-app-podcast/src/blossom.rs` → `host_op_publish::publish_episode`) to
  `nmp.blossom.upload` via `signer_pubkey` roster selection. **BLOCKED by #606:** the
  per-podcast NIP-F4 keys live in the Podcast-domain `PodcastKeyStore`, NOT in the
  NMP account roster (`ctx.identity`). `nmp.blossom.upload` with `signer_pubkey`
  only resolves accounts registered in the NMP kernel's identity roster. Hardening
  NIP-F4 publishing (issue #606) will register per-podcast keys as named roster
  accounts or establish an alternate signing seam. Until then, `blossom.rs` stays
  (it uses direct `Keys` signing which works without the roster).
- ~~**kernelsigner-deadcode-removal.**~~ DONE (PR `chore/kernelsigner-deadcode-backlog-truthfulness`).
  Deleted `KernelSigner` struct, `NostrSigner` protocol, and `NostrEventDraft` from
  `App/Sources/Services/Nip46/NostrSigner.swift`; removed the now-dead
  `signEventForReturn` chain from `KernelBridge.swift` + `KernelModel.swift`.
  `NostrSignerError` kept (used by `KernelBridge.swift` + `SignedEventsRegistryTests.swift`).
  `SignedEventsRegistry` + `nmp_app_sign_event_for_return` FFI kept (tested; D13 seam).
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
  one-entry JSON file under `App/Resources/changelog/` with a unique
  `shipped_at` timestamp. Do not edit a shared `whats-new.json`; that file is
  no longer the app's changelog storage shape.
- **docs-status-audit.** Every PR that changes a listed item must edit the
  existing backlog item instead of adding parallel state or leaving stale
  status behind.
- **line-limit-audit.** Continue enforcing the 300-line soft and 500-line hard
  limits. Split files before adding logic to near-limit modules.
  Current `ci/check-file-sizes.sh` passes on `main`, but raw line-count audits
  still surface large implementation files that should be split before they
  grow further, especially `AppStateStore+KernelActions.swift`,
  `AppStateStore+KernelProjection.swift`, `LivePodcastInventoryAdapter.swift`,
  `AgentTools+Podcast.swift`, and near-limit Rust/Kotlin modules.
  - ~~**appstatestore-split.**~~ RESOLVED. `App/Sources/State/AppStateStore.swift`
    is now 417 lines on origin/main HEAD 12874d7e — under the 500-line hard limit.
    The blocking in-flight branches have landed and the file is within policy.
    No further split required unless the file grows again.
  - **kernelprojection-split.** `App/Sources/Bridge/AppStateStore+KernelProjection.swift`
    was already over the 500-line hard limit (533 on origin/main; 603 after the
    `fix/incremental-episode-update` summary-level episode diff, which added the
    diff loop plus its reuse-invariant safety comment). Split deferred: this file
    is concurrently touched by the in-flight `fix/file-size-projection`,
    `fix/double-recompute`, and `signpost-instrumentation` branches, and a split
    now would conflict with all three. Relocate `toEpisode`/`toChapter` mapping
    and `mergeResolvedProfiles`/`backfillSyntheticEpisodes` into sibling files
    once those land. Owner: unassigned.
- **m1.6-kernel-widget-position.** Once `AudioCapability.sendReport` is wired
  to the Rust kernel (M1.6), kernel-projection position ticks will drive
  `nowPlaying.positionSecs`. At that point `PlatformCapability.applyNowPlayingSnapshot`
  needs a separate position-write path (not gated by the identity dedup) so the
  widget stays live during playback. Owner: M1.6 agent.
- ~~**episode-metadata-indexer-ownership.**~~ DONE. The Swift
  `EpisodeMetadataIndexer` file is gone in this worktree, and Rust
  knowledge indexing owns metadata coverage plus transcript chunking/search.
- ~~**feed-not-modified-rev-bump.**~~ Done in this PR. The shared feed response
  parser now returns a canonical cache for both `200` and `304` results,
  preferring response validators and falling back to prior validators when
  omitted. Ensure-feed refresh, kernel refresh, and async known-feed subscribe
  continuations persist that cache through `update_refresh_metadata`; `304`
  results update metadata without a snapshot revision bump.
- ~~**auto-advance-actor-stage-resilience.**~~ Fixed: `maybe_auto_advance`
  now acquires the actor lock atomically with staging — if staging fails (lock
  poisoned), Load+Play are not dispatched. `dispatch_audio_cmd` and
  `dispatch_download_cmd` gained D6 null-app guards (matching
  `PodcastHostOpHandler::dispatch_audio`). Three behavioral tests added to
  `audio_report_tests.rs`. PR: `fix/auto-advance-stage-divergence`.

## Pending Decisions

_All pending decisions resolved. See Done section for resolutions._

- **legacy-migration-delete.** Resolved on current `main`: there is no shipped
  v1 app and no legacy migration surface left to execute. The parked `ios/`
  shell is gone, `App/Sources/Capabilities/PodcastKeysKeychainMigration.swift`
  is absent, and `legacyIO` / `pcst.legacy_io.capability` no longer appear in
  the active source tree.

## Resolved Decisions

- **Podcast key storage.** `podcast-keys.json` is the canonical and final store for per-podcast NIP-F4 secrets. No Keychain. No migration. The M7 Keychain flip plan is cancelled.
- **Storage engine.** JSON is canonical for the podcast store and settings. `sqlite-vec` is used for RAG vector search. No sled/SQLite migration needed for the podcast store.
- **Relay publish queue semantics.** NMP owns relay publishing entirely — queue, retry, routing, and status. The app dispatches events to NMP and is not aware of WebSockets or relay state.
- **Provider availability.** Not a real pending decision — removed.

## Done / Recently Reconciled

- **voice-conversation-off-thread-dispatch-uaf.** Done on branch
  `fix/voice-conversation-uaf`. The original suggestion (route `Speak` back
  through the actor thread) was unreachable: pinned nmp-ffi rev `ec15ede`
  exposes no accessor to clone the capability-callback slot and no seam to
  post a closure onto the actor thread, and the dep must not be forked.
  Instead `VoiceConversationManager` now retains the outer turn `JoinHandle`s
  and exposes `shutdown()` (abort + `block_on(join)`); `PodcastApp.shutdown()` /
  `Drop` calls it before runtime teardown, so every in-flight `app` dereference
  is fenced before the allocation frees. A `Drop` impl on the manager alone
  could not serve as the fence because projection closures hold strong
  `Arc<PodcastHandle>` clones. A `shutting_down` flag makes any late
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
- ~~**home-inbox-projection-ownership.**~~ Done in this PR: Home's inbox hero
  cards and the full Inbox screen now consume `PodcastUpdate.inbox` instead of
  rebuilding inbox rows from Swift `Episode.triageDecision` / `triageIsHero`.
  Rust owns inbox eligibility, priority ordering, rationale text, and the Home
  Inbox header roll-up counts via `nmp_app_podcast_home_triage_rollup`; Swift
  only passes the active category renderer scope and resolves episode ids for
  native row rendering.
- ~~**home-continue-listening-kernel-ownership.**~~ Done in this PR:
  Continue Listening now calls `nmp_app_podcast_home_continue_listening`.
  Rust owns the unplayed/non-archived/started/two-week/category filter and row
  ordering; Swift resolves returned episode ids for native rendering. The
  Home subscription list now calls `nmp_app_podcast_home_subscription_list`;
  Rust owns followed-feed eligibility, active category scope, All/Unplayed/
  Downloaded/Transcribed filter semantics, and newest-episode ordering while
  Swift resolves returned podcast ids for native rendering. The
  recommended-picks rail also preserves Rust `PodcastUpdate.picks` ordering
  instead of re-sorting by score in Swift. CarPlay Listen Now now calls
  `nmp_app_podcast_carplay_listen_now`; Rust owns subscribed-show scope,
  archive visibility, in-progress/latest membership, ordering, and section
  caps while Swift only resolves ids and renders `CPListTemplate` rows.
  Home's category picker now calls `nmp_app_podcast_home_category_cards`;
  Rust owns valid/subscribed podcast membership and non-archived unplayed
  totals for category cards while Swift keeps the legacy category display
  model and native rendering. CarPlay Shows now calls
  `nmp_app_podcast_carplay_shows` and
  `nmp_app_podcast_carplay_show_episodes`; Rust owns followed-show ordering,
  show unplayed counts, per-show episode membership, archive visibility, and
  row caps while Swift resolves ids and renders native CarPlay rows. Library
  show detail now calls `nmp_app_podcast_library_show_episodes`; Rust owns
  show episode membership, archive visibility, newest-first ordering, and caps
  while Swift keeps only local search filtering and native row rendering. All
  Podcasts and Settings Subscriptions rows now call
  `nmp_app_podcast_library_podcast_stats`; Rust owns total episode counts while
  Swift formats native labels and confirmation copy. Agent empty-state
  suggestions now call `nmp_app_podcast_agent_empty_state`; Rust owns the
  resume/subscribed/onboarding context choice while Swift renders the selected
  copy. Agent subscription/delete/refresh/owned-podcast result counts now use
  the same Rust-owned podcast stats projection, and audio-URL-to-episode lookup
  now calls `nmp_app_podcast_library_episode_for_audio_url` instead of scanning
  Swift episode arrays. Home active-thread invalidation now calls
  `nmp_app_podcast_library_summary` for total unplayed count instead of
  reducing Swift `unplayedCountByShow`. The old Swift episode projection
  cache fields (`unplayedCountByShow`, `episodeIndexesByShow`,
  `inProgressEpisodesCached`, `recentEpisodesCached`, downloaded/transcribed
  sets) were removed; legacy helper APIs now resolve through Rust projections
  or remain as no-op mutation compatibility shims. Library All Episodes now
  calls `nmp_app_podcast_library_all_episodes`; Rust owns filter/search
  predicates, archive visibility, newest-first ordering, pagination, and total
  filtered counts while Swift resolves ids and renders rows. CarPlay Downloads
  now calls `nmp_app_podcast_carplay_downloads`; Rust owns downloaded
  membership, archive visibility, newest-first ordering, and caps while Swift
  formats native row details. `sortedFollowedPodcasts` now calls
  `nmp_app_podcast_library_followed_podcasts`; Rust owns followed-feed
  eligibility and alphabetical ordering while Swift resolves ids for Settings
  and OPML export. The legacy `sortedFollowedPodcastsByRecency` wrapper now
  composes Rust's followed-podcast and Home subscription-list projections
  instead of sorting subscriptions by episode dates in Swift. Agent-owned
  podcast lists and Settings badges now call
  `nmp_app_podcast_library_owned_podcasts`; Rust owns owner-marker eligibility
  and ordering while Swift resolves ids and renders settings/tool results.
  Category lists/details now call `nmp_app_podcast_library_categories`; Rust
  owns category ordering plus valid subscribed podcast membership/order while
  Swift keeps the legacy display/settings DTO and native rendering. Category
  management count labels now use the same Rust-projected valid membership
  instead of raw legacy `subscriptionIDs`. Downloads Manager now calls
  `nmp_app_podcast_library_download_rows`; Rust owns
  active/failed/downloaded membership and section ordering while Swift renders
  status details and actions. Settings download summary uses the same Rust
  download-row projection. Bookmarks now calls
  `nmp_app_podcast_library_starred_episodes` for starred membership/order and
  unions that with Swift-local clips/notes before rendering. Episode deep links
  now call `nmp_app_podcast_library_episode_lookup`; Rust owns matching URL
  episode references against canonical episode ids and publisher GUIDs.
  Category transcription aggregate state now rides the Rust category projection
  as `all_transcription_enabled` instead of scanning Swift kernel summaries,
  and category settings UI no longer writes or renders from the legacy
  `CategorySettings` DTO; that Swift state remains only as legacy migration
  input for old disabled transcription settings.
  Settings/Data Storage record counts and Home active-thread invalidation now
  use Rust `library_summary` episode/followed-podcast counts instead of
  `store.episodes.count` / `state.subscriptions.count`; category recompute and
  Home category-picker labels use the same Rust followed-podcast count.
  Home first-run and "See all podcasts" affordance logic now uses Rust
  `library_summary` followed/unfollowed facts instead of Swift subscription and
  podcast scans.
  Agent category inventory and category-change label fan-out now reuse the
  Rust category projection for ordering/membership instead of sorting or
  filtering Swift `subscriptionIDs`.
  Nostr discovery rows now call `nmp_app_podcast_library_subscription_status`;
  Rust owns matching feed URLs and owner pubkeys against existing followed /
  owned shows for the already-subscribed state.
  All Podcasts now calls `nmp_app_podcast_library_all_podcasts`; Rust owns
  Unknown-sentinel exclusion, search matching, and alphabetical ordering while
  Swift resolves ids and renders rows/delete copy.
  Podcast deletion now dispatches Rust unsubscribe and relies on the snapshot
  projection to remove podcast, follow, and episode rows; the dead Swift
  direct-follow helper and duplicate local unsubscribe deletion were removed.
  Feed subscribe now lets the Rust subscribe handler own duplicate-follow
  rejection; Swift only maps the kernel error back to existing UI copy.
  Per-show auto-download changes now dispatch only the Rust
  `set_auto_download` action and read the result back from projection instead
  of mutating Swift subscription policy state first.
  Mark-played, reset-progress, mark-unplayed, and starred episode actions now
  dispatch only Rust actions; Swift keeps the native position debounce cache
  clear but no longer performs duplicate optimistic episode flag writes.
  Dead Swift mutation helpers for persisted episode chapters and ad segments
  were removed; chapter fetch and AI chapter/ad compilation persist through
  Rust actions and project back into Swift.
  The Storage "Delete after played" toggle now updates via `updateSettings`,
  ensuring the Rust `set_auto_delete_downloads_after_played` action owns the
  persisted policy instead of a nested Swift settings binding.
  Transcript auto-ingest now calls
  `nmp_app_podcast_transcript_auto_ingest_candidates`; Swift supplies
  local-audio availability facts while Rust owns candidate eligibility,
  optional new-episode scoping, newest-first ordering, and max-count limiting
  using the same planner as single-episode ingest.
  Episode detail transcript warmup now calls the ingest service for idle
  episodes and lets the Rust planner decide publisher/STT/skip behavior instead
  of Swift pre-gating on publisher URL or Unknown-podcast external status.
  Data export preview counts now inject Rust-owned followed-podcast and episode
  totals instead of deriving those display facts from Swift subscription and
  episode DTO counts.
  Per-show new-episode notification toggles now dispatch
  `set_podcast_notifications_enabled`; Rust persists the per-podcast disabled
  set, filters notification capability dispatch during feed refresh, and
  projects `notifications_enabled` back onto subscription rows.
  Category-level auto-download/RAG/notification override controls were removed
  from active UI because they were legacy Swift-only DTO knobs with no
  Rust-owned runtime policy; only transcription remains visible because it is
  wired through Rust per-podcast transcription policy. Reintroduce those
  category controls only after Rust owns durable category policy without lossy
  fan-out.
  All Episodes and Downloads Manager row context now reuse the Rust all-podcasts
  projection instead of raw `state.podcasts` maps when resolving show metadata.
  Settings Storage now calls `nmp_app_podcast_storage_breakdown`; Swift only
  enumerates raw local download files as an OS capability while Rust owns the
  library join, orphan classification, total bytes, per-show grouping, episode
  dedupe, and row ordering.
  Agent transcript search now calls `nmp_app_podcast_knowledge_resolve_scope`;
  Rust owns raw scope UUID disambiguation against canonical episode/podcast
  state before the knowledge query runs.
  Nostr feedless subscribe confirmation now calls
  `nmp_app_podcast_library_podcast_for_owner_pubkey`; Rust owns matching the
  owner pubkey to the canonical podcast row while Swift waits and returns the
  rendered `Podcast`.
  Subscription categorization recompute now calls
  `nmp_app_podcast_library_categorization_prompt` and
  `nmp_app_podcast_library_categorization_parse`; Rust owns followed-podcast
  prompt construction, response schema, UUID validation, last-write-wins
  dedupe, generated category ids, generated timestamp, and model attribution
  while Swift only executes the async completion capability and persists the
  returned category DTOs.
  Agent category edits now call `nmp_app_podcast_library_category_change`;
  Rust owns category reference resolution, podcast/category validation,
  single-category move semantics, previous/target result fields, and the
  kernel label set while Swift persists the returned category DTOs and
  dispatches the returned labels.
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
- **kernel-speed-persistence-uitest (#547).** FIXED by #561: `SetSpeed` now
  calls `set_default_playback_rate` so the rate is written to `podcasts.json`
  and survives `--UITestSeedRelaunch`. `XCTSkip` removed from
  `testPlaybackSpeedPersists`.
- **simulator-download-trigger-coverage (#547).** `testDownloadEpisode`
  (in `AppUITests/Sources/DownloadUITests.swift`) asserts only the
  state-transition triggered by tapping Download on ep2, which uses a stub
  enclosure URL (`test.podcast.local`). The full end-to-end path — trigger →
  background URLSession → download completes → "Downloaded" label — requires
  either (a) a real CDN reachable from the simulator, or (b) a local HTTP stub
  server that serves the bundled test-episode.mp3 at the ep2 enclosure URL so
  downloads complete without external network. Option (b) is the correct
  follow-up: add a `WireMock`/`Swifter` in-process stub in UITestSeeder that
  binds to a loopback port and overrides the ep2 enclosure URL to point to it.
- **simulator-nostr-publish-coverage (#547).** End-to-end Nostr publish
  (NIP-F4 kind:10154) cannot be automated in the simulator: the test seeder
  does not inject a signing keypair (ephemeral identity), public relay access
  is unreliable in CI, and relay-event verification is asynchronous and
  environment-dependent. The automated smoke (`testNostrIdentityScreenReachable`
  in `AppUITests/Sources/NostrPublishUITests.swift`) proves the identity
  navigation path is reachable. Full publish sign-off follows the manual
  protocol in that file: create a keypair in Settings → Identity, subscribe a
  podcast, publish via Show options → "Publish to Nostr", verify the event on a
  public relay (e.g. nostrudel.ninja). Fake-passing stubs are explicitly
  excluded by #547.
- **simulator-auto-download-trigger-coverage (#547).** `testAutoDownloadPolicyUIPath`
  (in `AppUITests/Sources/AutoDownloadUITests.swift`) verifies the full UI path
  for setting the auto-download policy. Observing an actual triggered download
  (enabling a rule and seeing a new episode download) requires a feed-refresh
  that returns a previously-unseen episode — not feasible deterministically in
  CI (depends on external network and a live feed returning new content). A
  follow-up integration test would seed a fake local RSS feed response with a
  new episode and observe the auto-download trigger; this is out of scope for
  #547 and requires a local HTTP feed server in the test harness.
