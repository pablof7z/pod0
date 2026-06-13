# Backlog

This is the tactical queue for active work, follow-ups, and pending decisions.
Do not duplicate these items in `WIP.md`; `WIP.md` only records branches and
worktrees currently in flight.

## Active P0 - Correctness Before More Features

- ~~**p0-nipf4-wire-contract.**~~ Done in PR #89: aligned kind `10154`/`54`
  builders and parsers with the NIP-F4 wire contract; removed non-NIP-F4
  `d`/`a`/`published_at`/`imeta` tags; round-trip tests verify absence.
- ~~**p0-nipf4-real-keys.**~~ Done: file-backed persistence to `podcast-keys.json` (atomic write/rename), reload on restart, key cleanup on `remove_owned_podcast`. Keychain migration deferred indefinitely — file storage is the canonical path.
- ~~**p0-nipf4-sign-and-publish.**~~ Done: `sign_event` produces real secp256k1-signed events with valid `id`/`pubkey`/`sig`; `dispatch_nostr_relay` publishes to `relay.primal.net` and returns `"published"` on relay acceptance. Relay URL is hardcoded but matches the only configured write relay. `relay_pending` status removed.
- **p0-nipf4-relay-discovery.** kind:10154 show discovery via relay IS wired (`NostrDiscoveryObserver` + `EnsureInterest` pattern; relay pool, not HTTP socket). Remaining: kind:54 episode fetch by podcast pubkey via relay (Nostr-only podcasts without RSS). HTTP gateway search remains a convenience path.
- ~~**p0-nipf4-author-claim.**~~ Done: `publish_author_claim` signs with active agent key and publishes kind:10064. Called after create/update/delete of owned podcasts.
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

- **inbox-triage-on-async-subscribe.** DONE (PR #383 deferred; completed in
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
- ~~**owned-podcast-episode-backfill-kernel.**~~ Done: kernel `update_owned`
  now detects a private→public flip and calls `publish_episode` for every
  episode atomically; the Swift loop deleted (PR #396).
- ~~**compat-service-stubs-delete.**~~ REMOVED — referenced path does not exist at origin/main.
- ~~**compat-domain-stubs-delete.**~~ REMOVED — referenced path does not exist at origin/main.
- ~~**compat-kernelmodel-delete.**~~ REMOVED — referenced path does not exist at origin/main.
- ~~**compat-useridentity-delete.**~~ REMOVED — referenced path does not exist at origin/main.
- **identity-kernel-actions.** Implement Rust-owned identity actions:
  import nsec, generate, clear, publish profile, connect/disconnect remote
  signer, connect via nostrconnect, cancel handshake, and expose fingerprint
  data in `AccountSummary`.
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
  OpenRouter key validation through JNI. Swift live
  wiki/title/categorization/chapter/clip completion callers no longer
  preflight OpenRouter/Ollama Keychain keys before invoking the shared Rust
  provider transport, and Swift OpenRouter settings validation no longer
  preflights Keychain before calling the shared validator. Swift Episode
  Diagnostics no longer hides the OpenRouter Whisper retry path behind a
  Keychain preflight; forced OpenRouter Whisper retries now call the shared
  Rust STT transport so missing-key/provider errors come from the backend.
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
  Scribe/AssemblyAI/online-search JNI calls, and exposes STT/TTS model
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
  variant-backed `AgentTaskIntent` payload, and the parked `ios/Podcast` shell's
  task sheet mirrors the same typed creation path. Swift's scheduled prompt
  tool/settings surface now creates and edits `agent_prompt` tasks through
  `podcast.tasks` typed intents, renders the shared `agentTasks` projection,
  and dispatches Rust-owned `run_due` on foreground instead of scanning a
  persisted Swift task array. Agent task rows now persist through the shared
  Rust sidecar so disabled, edited, deleted, and completed tasks survive kernel
  restarts across iOS, Android, and TUI. Keep raw `create` as
  compatibility/internal only; remaining work is a durable background-agent
  execution/history model for prompt tasks if they should remain isolated from
  the main agent chat.
- **relay-list-ownership.** Replace `@AppStorage("nip65.relays")` seed state
  with NMP relay-list store reads/writes and real NIP-65 publish/refresh flow.
  Rust prerequisite SHIPPED (`feat/podcast-relay-ops`): `configured_relays`
  projection on `PodcastUpdate` + `add_relay`/`remove_relay`/`set_relay_role`
  ops on `podcast.settings`. iOS App Relays editor now unblocked.
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
- **app-relays-config-ui.** DONE (`feat/app-relays-ui`). The App Relays editor
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
- **snapshot-push-delivery.** Replace the remaining 500 ms polling dependency
  with push-style delivery through the NMP update sink for autonomous changes,
  while keeping content-hash throttling for volatile playback/download fields.
- **capability-router-unify.** Collapse `SyncCapabilityBridge` and
  `PodcastCapabilities.shared.handleJSON()` into one routing contract. Each
  capability owns its threading; Rust has one path to dispatch commands and
  receive reports.

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
- **remote-command-kernel-routing.** Lock-screen / Control Center commands
  (`AudioCapability+RemoteCommands`) call `execute(.play)`/`.seek` which run the
  engine directly through the same `commandHandler` that Rust-issued commands
  use. After a cold restart where the player restored a paused episode but Rust
  never staged it, a lock-screen Play starts audio without a `kernelLoad`, so
  Rust has no `episode_id` for the subsequent position reports. Fix by routing
  lock-screen-originated commands through a report-to-Rust path (or staging the
  restored episode in Rust on restore) — distinct from Rust-issued playback
  commands so it doesn't loop through `handle_load`'s echoed `Load`.
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

- ~~**social-bunker-signing-kernel.**~~ DONE (D13, PR to be merged). Both
  `.localKey` and `.remoteSigner` (NIP-46 bunker) identities publish through
  `podcast.social` → kernel `publish_unsigned_event` → `sign_active_nonblocking`
  → `PendingSign` park for remote signers. There was never a Swift NIP-46 signing
  branch in `UserIdentityStore+Publishing.swift`; the stale comment that claimed
  one was removed in this PR. Verified against NMP v0.6.2 rev 6418a7ac
  `crates/nmp-core/src/actor/commands/publish.rs` + `pending_sign.rs` +
  `nip46_bunker_signing.rs` integration test.
- **nip73-formatting-kernel.** `publishUserClip` dispatches typed fields to
  `podcast.social publish_highlight`; the kernel's `build_highlight_tags`
  assembles the NIP-73/84 tag set (enclosure/feed `r`, episode `i` coord,
  `context`, `alt`) from those fields. Tag formatting is already kernel-owned
  (#355). Low priority follow-up: pass raw episode/podcast identifiers so Swift
  never formats the `i podcast:item:guid:<guid>#t=<start>,<end>` string.
- **social-publish-relay-target.** Kernel social publishing (kind:1, kind:1111, kind:10064)
  routes via `nmp_dispatch.rs` with `target: Auto`, which uses NMP's relay pool strategy
  (see `publish_raw_via_nmp` line 55–72). Doc comment at `host_op_publish.rs:169` is
  stale (mentions `relay.primal.net`); actual routing is pool-aware. Verify whether
  `Auto` target respects user's configured write relays or remains pool-only.
- ~~**episode-comments-relay-wiring.**~~ DONE (verified at `apps/nmp-app-podcast/src/comments_handler.rs`).
  Real kind-1111 relay subscribe/publish is wired: `handle_fetch_comments` 
  (line 61) subscribes via `push_interest_via_nmp` with kind:1111 + NIP-73 `#i` tag filter;
  `handle_post_comment` (line 103) publishes via `publish_raw_via_nmp` (line 140);
  `CommentsObserver` (line 164) receives inbound events and caches by episode.
  Episodes are mapped to anchors via `episode_nip73_anchor` (line 70, 129).
- **social-graph-store-wiring.** ~~Replace `social_handler.rs` `nostr_pending`
  with NMP kind:3 contact-list store reads, kind:0 metadata hydration,
  subscription refresh, and snapshot updates.~~ **CLOSED** — replaced by reactive
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
    (`KernelModel+Social.swift`); `AgentAccessControlView` routes through
    kernel; dead `NostrPendingApproval` / `NostrApprovalPresenter`
    scaffolding deleted. Follow-ups: bridge-decode fixture test for
    `trusted` field; Android access-control UI.
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
  - **OPEN — LLM responder loop.** The Swift `NostrAgentResponder` was deleted
    in PR #248 (kernel-owned signing / D13 migration). The inbound→model→outbound
    autopilot (dedup via responded-event ids, per-root outgoing turn cap,
    `wtd-end` end-conversation gate, bounded kind:0 profile hydration,
    owner-consult `ask` tool) must be re-implemented in the kernel. A parallel
    PR `feat/kernel-kind1-auto-responder` is implementing this restoration.
  - Non-goal: NIP-17 (private direct messages) is out of scope for agent
    coordination and will not be used for this purpose.

## Active P1 - AI Scaffold Replacement

- **episode-pipeline-followups.** Deferrals from the kernel-owned episode
  pipeline event-log + auto-download work (`feat/episode-pipeline-events`):
  1. **auto-download mode collapse.** The kernel stores auto-download as a
     single `enabled` bool, collapsing `AutoDownloadPolicy.Mode.latestN(N)` and
     `.allNew` (Swift `AutoDownloadPolicy.swift`). The new
     `auto_download_backfill_candidates` scan therefore treats every enabled
     show as "keep the latest `AUTO_DOWNLOAD_BACKFILL_LIMIT` (=3) undownloaded
     episodes" rather than honoring a user-chosen N or true all-new. Follow-up:
     project the mode + N into the kernel store so backfill respects it.
  2. ~~**ad detection not in the kernel.**~~ DONE (PR refactor/kernel-ai-chapters-ad-spans).
     `ai_chapters_llm.rs` now emits ad spans; `ai_chapters.rs` persists via
     `set_ad_segments_for` and emits `ads.ready`. `AIChapterCompiler.swift` deleted.
  3. ~~**AI chapters not reported to the kernel from the legacy path.**~~ DONE (same PR).
     All call sites dispatch `podcast.chapters.compile`; the Swift writer is removed.
- **inbox-triage-progress-projection.** ~~Swift shimmer done~~ The
  `inbox_triage_in_progress` bool is projected onto `PodcastUpdate` and
  `HomeFeaturedSection.isStreaming` is now wired to it (Fix B, PR #TBD). The
  "triaged Xh ago" subtitle (`lastTriagedAt`) is still pending: it requires
  projecting `inbox_last_triaged_at: Option<i64>` from the triage cache
  timestamp (`host_op_handler.rs`) onto `PodcastUpdate`, then passing it
  through `HomeView` → `HomeFeaturedSection.lastTriagedAt`.
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

  Remaining follow-ups (now tracked on the surviving Swift path, not this
  deleted stub): NIP-F4 publishing of agent episodes, deletion cleanup of
  `agent-episodes/<id>.m4a`, and verifying the published `.synthetic` episode
  metadata round-trips the store's disk layer across restart.

  Projection gap resolved: generated episodes and unfollowed external RSS
  ensure now ride the Rust projection. Keep this TTS item scoped to surviving
  Swift composer follow-ups
  (NIP-F4 publishing, deletion cleanup, restart verification), not feed-store
  ownership.
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
- **android-tier1-parity.** Finish Android parity gaps that still require
  user-visible work or policy validation: lock-screen MediaSession commands
  routed through Rust playback policy, Android keypair generation, and Tier 2+
  AI/Nostr/platform surfaces. Tier 1 library, search, subscribe, feed refresh,
  playback, sleep timer, playback queue, downloads, settings, BYOK nsec import,
  HTTP capability execution, and audio report round-trips now use the NMP
  kernel/capability path. Android can call the shared Rust provider
  complete/embed/catalog/image/rerank, online search, and cloud-STT transports
  through JNI, and model-role settings now load the shared Rust catalog for
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
- **tui-mpv-position-sampling.** DOCUMENTED EXCEPTION + tracked follow-up
  (#322). The terminal player samples mpv's `playback-time` over the JSON IPC
  socket every 250 ms (`AudioHost::poll_position`, driven off the UI animation
  frame clock in `apps/podcast-tui/src/main.rs`). libmpv / mpv IPC expose no
  per-frame position event, so periodic sampling is the only mechanism the
  player offers (the sampling cadence is the legitimate exception). The
  previous fake-progress path (incrementing the position by the tick interval
  when no mpv backend was present) was removed: with no real backend the
  position is now left unknown/unchanged rather than fabricated.
  FOLLOW-UP (not done in #322): the sampled position is currently stored in
  `last_position_secs` and NOT forwarded to the kernel — there is no
  `nmp_app_podcast_audio_report` call anywhere in `apps/podcast-tui`, so live
  mpv progress never reaches the kernel projection. Wiring that single FFI
  report (so the TUI surfaces real playback progress, the way iOS/Android do
  via `AudioReport::Playing`) is the remaining work. The TUI is a secondary
  target; correctness over polish, so this PR fixes the fabrication and
  documents the gap rather than building the report path.

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
- **m5-non-utf8-feed-bodies.** Widen HTTP capability body transfer to preserve
  non-UTF8 feed bytes. Update Swift and Rust so XML encoding declarations are
  honored.
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
  `BlossomUploader.swift` is deleted. The `nmp-blossom` action module (v0.6.0) owns
  the full Build → Sign → Transport pipeline (D13/D0).
- **blossom-audio-path-migration.** Migrate the audio upload path
  (`apps/nmp-app-podcast/src/blossom.rs` → `host_op_publish::publish_episode`) to
  `nmp.blossom.upload` via `signer_pubkey` roster selection. **BLOCKED:** the
  per-podcast NIP-F4 keys live in the Podcast-domain `PodcastKeyStore`, NOT in the
  NMP account roster (`ctx.identity`). `nmp.blossom.upload` with `signer_pubkey`
  only resolves accounts registered in the NMP kernel's identity roster. This
  requires registering per-podcast keys as named roster accounts, or an alternate
  signing seam. Until that capability lands, `blossom.rs` stays (it uses direct
  `Keys` signing which works without the roster).
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
  `shipped_at` entry in `App/Resources/whats-new.json` and the mirrored iOS
  resource if still required by project shape.
- **docs-status-audit.** Every PR that changes a listed item must edit the
  existing backlog item instead of adding parallel state or leaving stale
  status behind.
- **line-limit-audit.** Continue enforcing the 300-line soft and 500-line hard
  limits. Split files before adding logic to near-limit modules.
  - **appstatestore-split.** `App/Sources/State/AppStateStore.swift` is already
    over the 500-line hard limit (583 on origin/main; 602 after the
    `fix/triage-counts-cache` triage-bucket stored properties, which *cannot*
    move — Swift stored properties must live in the class body, not an
    extension). The split must relocate *methods* (not the projection-cache
    stored props) out of the main file. Deferred to avoid conflicting with the
    in-flight `fix/double-recompute`, `file-size-projection`, and
    `signpost-instrumentation` branches that all touch this file. Owner: unassigned.
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
- **episode-metadata-indexer-ownership.** `App/Sources/Services/EpisodeMetadataIndexer.swift`
  is a Swift-owned RAG embedding-backfill service that holds native batching and
  rate-limit *policy*: it chunks pending episodes at `batchSize = 32` and sleeps
  `interBatchDelayNanoseconds = 200_000_000` (0.2s) between batches
  (`:53`/`:57`, applied in the backfill loop at `:112`–`:119`). Per D7 the
  rate-limit / batch policy belongs in the Rust kernel, which already owns the
  providers and the episode store; per D4 the "which episodes are indexed" fact
  is a kernel write (`MarkEpisodesMetadataIndexed` action exists), yet the
  selection/ordering policy feeding it is still native. Decision needed: migrate
  the backfill orchestration into a kernel action/job (kernel owns batch size,
  inter-batch pacing, and the pending-episode scan), or write an ADR documenting
  why the embeddings utility is intentionally shell-owned. Surfaced in the
  2026-06-11 NMP architecture audit.
- **provider-credential-connected-at-kernel-time.** Android
  `android/Podcast/app/src/main/java/io/f7z/podcast/ProviderCredentialActions.kt`
  stamps the credential `connectedAt` field natively: `epochSeconds()` (`:286`,
  `System.currentTimeMillis() / 1000`) is read at `:73`, `:119`, `:163`, `:209`,
  and `:255` and passed through the credential payloads into the kernel. D9 says
  the kernel owns time, so the kernel should stamp `connectedAt` on receipt and
  the field should be dropped from the shell payloads entirely. When picked up,
  check whether iOS does the same for the equivalent provider-credential ops and
  fix both shells together so the native-clock value cannot re-enter from either
  side. Surfaced in the 2026-06-11 NMP architecture audit.
- **feed-not-modified-rev-bump.** `apps/nmp-app-podcast/src/host_op_handler/podcast_actions_feed.rs`
  (`:175`–`:182`) handles a `304 NotModified` feed refresh by updating
  etag/last-modified via `update_refresh_metadata` and then unconditionally
  `self.rev.fetch_add(1, ...)`. That rev bump forces a full snapshot rebuild +
  FFI decode on every shell for a tick where nothing user-visible changed —
  multiplied across N feeds on a refresh-all, this is pure main-thread churn
  (compare the snapshot-decode hot-path entries). Fix: skip the bump when only
  refresh metadata changed (etag/last-modified are not projected to the shells),
  or route conditional-GET metadata through a metadata-only path that does not
  participate in the snapshot rev. Surfaced in the 2026-06-11 NMP architecture
  audit.
- **auto-advance-actor-stage-resilience.** In
  `apps/nmp-app-podcast/src/ffi/audio_report.rs`, `maybe_auto_advance`
  (`:263`–`:307`) pops the next episode from the canonical queue and resolves its
  playback info, then stages it on the player actor under
  `if let Ok(mut actor) = handle.player_actor.lock() { actor.stage_load(...) }`
  (`:280`). On a lock failure (poison-only, near-theoretical) the `stage_load`
  is silently skipped, but the `Load` + `Play` dispatch at `:285`–`:289` still
  fire — leaving the actor with no staged record for the now-playing episode.
  That is the same symptom class as the fixed lock-screen-play bug: position
  never persists and the episode is never marked played. Cheap hardening:
  acquire the actor lock before popping the queue, or fold the staging into the
  same lock acquisition as the `auto_play_next` read at `:245`, so the staged
  record and the dispatched Load can never diverge. Surfaced in the 2026-06-11
  NMP architecture audit.

## Pending Decisions

_All pending decisions resolved. See Done section for resolutions._

- **legacy-migration-delete.** All "legacy migration" infrastructure is dead —
  there is no shipped v1 app and no users to migrate. Delete:
  `App/Sources/Capabilities/PodcastKeysKeychainMigration.swift`,
  `ios/Podcast/Podcast/Capabilities/LegacyKeychainMigration.swift`,
  `ios/Podcast/Podcast/Capabilities/LegacyIOCapability.swift`,
  `ios/Podcast/Podcast/Capabilities/LegacyIOTypes.swift`.
  Remove the `legacyIO` field + routing from `PodcastCapabilities.swift`,
  the `runIfNeeded` call from `KernelBridge+Callbacks.swift:78`, and any
  `pcst.legacy_io.capability` handling on the Rust side. Remove all file
  references from `project.pbxproj`.

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
