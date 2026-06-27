# Migration v2 — Rust kernel becomes the source of truth

**Status:** Active. **Owner:** principal engineer. **Linked from:** `docs/plan.md`.

This plan replaces the ad-hoc "lane" work with a single ordered milestone
sequence that ends with `App/Sources/` deletable and the Rust
`nmp-app-podcast` kernel owning every business decision. Per-feature parity remains tracked in
`docs/plan/nmp-feature-parity.md`; this document tracks the **plumbing** that
unblocks that matrix.

## Doctrine recap (load-bearing)

- **D0** Rust decides; Swift renders + dispatches. No iOS-side policy.
- **D6** Errors are data on the snapshot, never thrown out of FFI.
- **D7** Capabilities report results; they do not decide what to do next.
- **No Swift-only domain state** preserved across projection passes.

## Top-level architectural decisions to lock in

These are not negotiable later in the plan — answer them in M0.

1. **One iOS runtime path wins.** `App/Sources/**` is the Tuist target
   (`Project.swift`, 633 Swift files, `@main` in `App/Sources/AppMain.swift`).
   The parked `ios/Podcast/Podcast/` tree has been deleted from current
   `main`, so Option B is now settled: continue refactoring `App/Sources/**`
   in place toward the thin-shell shape until parity exits prove it can be
   removed. Rationale: `App/Sources/` ships today and contains the unique
   reference behavior; rebuilding a deleted parked shell would add risk and
   rework.
2. **Audio stays in Swift, behind `AudioCapability`.** `AudioEngine` (AVPlayer
   wrapper) is owned by `PodcastCapabilities.audio` and driven by
   `AudioCommand`s emitted by Rust's `PlayerActor`. `PlaybackState` must read
   from `PodcastUpdate.nowPlaying`, not from `engine.currentTime`.
3. **Rust owns position writes.** Position reports flow `AudioCapability →
   PlayerActor → snapshot`. Swift's `positionCache` /
   `flushPendingPositions` / 1 Hz tick loop is deleted in M1.
4. **No new top-level planning files.** This file lives under `docs/plan/`
   per `AGENTS.md`. Add one row to the `docs/plan.md` table linking here.

---

## Milestone M0 — Lock the destination

**Done when:** `docs/plan.md` references this file, the Option B decision is
recorded in this section, and `WIP.md` shows no agents touching either
`PlaybackState.swift` or `EpisodeDownloadService*.swift` (so M1+M2 can start
without conflict).

Tasks:

1. Add a row to the table in `docs/plan.md` pointing at
   `docs/plan/migration-v2.md`.
2. ~~Tag every `ios/Podcast/Podcast/Compat/*.swift` file with a one-line
   "delete after Mn" comment so accidental new references are obvious in
   review.~~ Not applicable on current `main`: the `ios/` tree is gone.
3. Drain in-flight branches touching `PlaybackState` / `EpisodeDownloadService`
   before opening any M1/M2 PR.

Status: **done**. `docs/plan.md` links this file, Option B is recorded against
the current `App/Sources/**` target, and the parked `ios/` tree no longer
exists.

---

## Milestone M1 — `PlaybackState` becomes a pure renderer

**Done when:** `App/Sources/Features/Player/PlaybackState.swift` is **≤ 300
lines** (current: 516), contains **zero** business callbacks, **zero**
persistence loop, **zero** end-detection, **zero** auto-skip-ads logic,
**zero** widget writes, and **zero** queue mutations. The only thing it does
is forward UI gestures to `kernel.dispatch(...)` and observe
`PodcastUpdate.nowPlaying`.

This is the largest milestone. **Order matters within it** — every Swift
deletion below is only safe once the matching Rust projection / handler
exists. The substeps below are listed in the order they must land.

### M1.1 — Extend `PodcastUpdate` so Swift no longer needs a 1 Hz loop

Status: **done** (PR feat/m1-playbackstate-pure).

Rust changes in `apps/nmp-app-podcast/src/ffi/projections.rs` (or the
matching projection file) and `apps/nmp-app-podcast/src/ffi/snapshot.rs`:

- `PlayerState`: add
  - `sleep_timer: { mode: "off" | "duration" | "end_of_episode", remaining_secs: f64 }`
  - `did_reach_natural_end: bool`
  - `segment_end_secs: Option<f64>` (for bounded agent segments)
  - `current_chapter_title: Option<String>`
  - `current_chapter_artwork_url: Option<String>`
- `SettingsSnapshot`: add
  - `auto_play_next: bool`
  - `auto_mark_played_at_end: bool`
  - `headphone_double_tap_action: String` (raw value of
    `HeadphoneGestureAction`)
  - `headphone_triple_tap_action: String`

Mirror in `App/Sources/Bridge/Generated/PodcastUpdate.generated.swift`. Do
**not** ship the Swift changes until the Rust side compiles and round-trips
through `actions_tests.rs`.

### M1.2 — Add the player ops that the deletions below depend on

Status: **done** (PR feat/m1-playbackstate-pure). New action variants in
`apps/nmp-app-podcast/src/ffi/actions/player_module.rs`:

- `Advance` — drop the current item, advance to the next queued item, no
  re-implementation by Swift. Replaces `playbackState.playNext { ... }`.
- `EndOfEpisode { mark_played: bool }` — Rust receives this from
  `AudioCapability` natural-end reporting (already exists today via
  `PlayerActor`); the iOS side does not synthesize it.
- `PersistPosition { episode_id, position_secs }` — fallback for routes
  that need to write a position outside an audio report (deep-link warm
  resume, mini-player restore). Most positions still flow via the audio
  capability report path.

New action variants in
`apps/nmp-app-podcast/src/ffi/actions/settings_module.rs`:

- `SetAutoPlayNext { enabled: bool }`
- `SetAutoMarkPlayedAtEnd { enabled: bool }`
- `SetHeadphoneGestureActions { double_tap: String, triple_tap: String }`

### M1.3 — Move business logic INTO `PodcastHostOpHandler`

Status: **partial** (PR feat/m1-playbackstate-pure; end-of-episode + auto-advance wired; segment-end, triage-clear-on-play, download-enqueue-on-play remain). Files:
`apps/nmp-app-podcast/src/host_op_handler.rs` and
`apps/nmp-app-podcast/src/host_op_player.rs` (split if needed under the
500-line limit).

Logic to add:

- **End-of-episode handler.** On `AudioReport::DidReachNaturalEnd`:
  1. If `settings.auto_mark_played_at_end`, mark episode listened
     (`InboxAction::MarkListened` internal call).
  2. If sleep timer was armed end-of-episode, fire the timer (stop playback,
     clear next-up).
  3. Else if `settings.auto_play_next` and queue is non-empty, pop and play
     the next item (reuse `PlayerAction::PlayNext`).
- **Segment-end handler.** When playhead crosses `segment_end_secs`:
  advance the queue. No-op if queue is empty (pause).
- **Auto-skip-ads.** On every position report tick, if
  `settings.auto_skip_ads_enabled` is on and the position is inside an
  `AdSegment`, dispatch a `Seek` to `segment.end_secs + 0.5`. Throttle to
  once per segment per session (the kernel already has the segment-id set
  from M3.B-era code; reuse it).
- **Triage clear-on-play.** Already wired via `InboxAction::ClearTriage`
  on the Rust side; on `PlayerAction::Play`, if the episode is
  `triage_archived`, call the internal clear before issuing the audio
  load.
- **Download enqueue on play.** On `PlayerAction::Play`, if the episode
  download state is `NotDownloaded` or `Failed`, enqueue a download in
  the same dispatch (no separate Swift trigger).

### M1.4 — Move widget writes off the 1 Hz loop

Status: **done** (PR feat/m1.4-widget-position-to-platform-capability).

Moved the 5-tick throttled `NowPlayingSnapshotStore.updatePosition` call from
`PlaybackState+AudioCallbacks.onPlayingTick` into a new
`PlatformCapability.applyPositionTick` method, wired via
`AppStateStore.onPositionTick` → `AppMain`. Deleted `widgetPositionTickCount`
from `PlaybackState`. The `NowPlayingSnapshot` position is now fully owned by
`PlatformCapability` on both the full-snapshot path (`applyNowPlayingSnapshot`)
and the position-only path (`applyPositionTick`).

### M1.5 — Delete the Swift business callbacks

Status: **todo**. Edit `App/Sources/App/RootView+Setup.swift` and
`App/Sources/Features/Player/PlaybackState.swift`. Remove these
callbacks in this order (each one requires the matching Rust handler from
M1.2/M1.3 to be live first):

| Callback | Removed when M-step lands |
|---|---|
| `onEpisodeFinished` | M1.3 end-of-episode handler |
| `onSegmentFinished` | M1.3 segment-end handler |
| `onClearTriageDecision` | M1.3 triage clear-on-play |
| `onEnsureDownloadEnqueued` | M1.3 download-enqueue-on-play |
| `onPersistPosition` + `onFlushPositions` | M1.6 (position cache deletion) |
| `onKernelEnqueueLast/Next/Dequeue/ClearQueue` | Already wired — remove the Swift-side bookkeeping that mirrors them in `PlaybackState.queue` and read the queue from `PodcastUpdate.queue` instead |
| `resolveShowName` / `resolveShowImage` / `resolveNavigableChapters` / `resolveActiveChapterTitle` / `resolveArtworkURL` | M1.1 (`current_chapter_*` projection adds these to nowPlaying) |
| `autoMarkPlayedOnFinish`, `autoSkipAdsEnabled`, `adSegments`, `headphoneDoubleTapAction`, `headphoneTripleTapAction` mirror fields | M1.1 + M1.3 |
| `applyPreferences(from: settings)` | M1.1 — `PlaybackState` reads settings off the snapshot directly |

The `Up Next` queue seed dance in `RootView+Setup.swift:46-55` and the
`pendingKernelQueue` field on `AppStateStore` go away once
`PlaybackState.queue` becomes `PodcastUpdate.queue` (no Swift-side
storage). Drop `onQueueFromKernel` and `pendingKernelQueue`.

### M1.6 — Delete the Swift position cache

Status: **todo**.

- Delete `App/Sources/State/AppStateStore+PositionDebounce.swift` and
  the matching stored properties (`positionCache`, `positionFlushTask`,
  `lastPositionFlush`) on `AppStateStore`.
- Delete `setEpisodePlaybackPosition` / `flushPendingPositions` from
  `AppStateStore`. The matching helpers in `RootView+Setup.swift` go
  away with the callbacks in M1.5.
- Wire `AudioCapability` to emit position reports → kernel updates
  `PlayerState.position_secs` → projection writes
  `Episode.playbackPosition`. (`AudioCapability` already reports; verify
  the kernel actually persists at the configured cadence and add a test
  if missing.)
- Delete the `backgroundObserver` flush hook on `AppStateStore` — Rust
  flushes its own state on `lifecycleBackground()` already.

### M1.7 — Verify

Status: **todo**.

- `wc -l App/Sources/Features/Player/PlaybackState.swift` is ≤ 300.
- `App/Sources/App/RootView+Setup.swift` contains no business callbacks
  (only haptic + UI-only wiring).
- `cargo test -p nmp-app-podcast` passes (especially
  `actions_tests.rs`, `player_module_tests.rs`,
  `host_op_handler_tests.rs`).
- Focused `xcodebuild test` on the player test bundle passes.
- Add `App/Resources/changelog/<UTC>.json` entries only for user-visible
  deltas (most of M1 is internal and needs none).

---

## Milestone M2 — Download path unification

**Done when:** the Tuist target builds with `DownloadCapability` as the only
download-transfer owner, the background URLSession handoff in
`AppDelegate.swift` forwards to `PodcastCapabilities.shared.download`, and any
remaining Swift file lookup helpers are thin storage/capability adapters rather
than download policy owners.

Subtask order:

1. **Verify the kernel's queue projection is complete.** Confirm
   `PodcastUpdate.downloads` exposes the fields the existing UI reads
   (`progress`, `expectedBytes`, `active`, `queuedCount`,
   `completedToday`, plus paused / failed counts). Add any missing fields
   to `DownloadQueueSnapshot` in
   `apps/nmp-app-podcast/src/ffi/snapshot.rs` and the Swift mirror in
   `App/Sources/Bridge/Generated/PodcastUpdate.generated.swift`.
2. **Move auto-download evaluation to Rust.** New action:
   `podcast.auto_download.evaluate` (or fold into the existing
   `podcast.refresh_all` post-step). New capability report:
   `nmp.network.is_on_wifi` so Rust can honour `wifi_only` policies
   without iOS deciding. Implementation lives in a new
   `apps/nmp-app-podcast/src/auto_download.rs` keeping the per-podcast
   policy from `EpisodeDownloadService+AutoDownload.swift` line-by-line.
3. ~~**Redirect the background URLSession handoff.** Edit
   `App/Sources/App/AppDelegate.swift:57-68` to call
   `PodcastCapabilities.shared.download.handleEventsForBackgroundURLSession(...)`.
   `DownloadCapability` already owns a background-aware `URLSession` —
   verify the session identifier matches.~~ Done on current `main`.
4. ~~**Remove every reference to `EpisodeDownloadService` from
   `App/Sources/`.**~~ Done on current `main`: no
   `EpisodeDownloadService*.swift` files or live references remain.
5. Audit `EpisodeDownloadStore` readers. It still backs local-file lookup for
   audio, transcript ingest, clip export, storage settings, UITest seeding, and
   generated TTS media. Keep it only as a storage helper or fold those reads
   into `DownloadCapability`/kernel-projected local paths when the call sites
   can be made policy-free.

Status: **partial**. The legacy service owner is gone and background handoff
routes through `DownloadCapability`; remaining work is storage-helper cleanup,
offline-first playback validation, background restore validation, and deletion
failure coverage.

---

## Milestone M3 — Settings, providers, and credentials in Rust

**Done when:** No `@AppStorage` reads or writes for domain settings exist in
`App/Sources/`. OpenRouter and Ollama credentials live in
`PcstIdentityCapability` keyring slots, not in
`OpenRouterCredentialStore`. `Settings.swift` is a thin codable projected
from `SettingsSnapshot`; `iCloudSettingsSync` becomes a capability that
reports KVS changes to the kernel, never the reverse.

Subtasks:

1. **Expand `SettingsSnapshot`** with every field currently on
   `App/Sources/State/Settings.swift` that is not already projected.
   Catalog before changing: skip intervals, auto-play-next, auto-mark, ad
   skip, notification quiet hours, default playback rate, transcript
   provider choice, agent provider choice, etc.
2. **Add `podcast.settings.*` ops** for every field.
3. **Replace `@AppStorage` writes** with `kernel.dispatch(namespace:
   "podcast.settings", ...)`. Reads come from
   `state.settings` (which is projected). Migrate `iCloudSettingsSync`
   into `PodcastCapabilities.iCloudSync` as a one-way report path
   (`nmp.icloud_kvs.changed`) — the kernel decides whether to accept the
   inbound value and writes back via its normal settings ops if it does.
4. **Move OpenRouter / Ollama credentials to Keychain.** Replace
   `OpenRouterCredentialStore` with calls into
   `PcstIdentityCapability` keyring slots. Keep
   `migrateLegacyOpenRouterSecretIfNeeded` as the one-shot migrator but
   sink its writes into a kernel `podcast.identity.set_provider_credential`
   op rather than directly into the legacy store.
5. **Delete `legacyOpenRouterAPIKey`** from `Settings.swift` after the
   migrator has run on every active install (a one-release deprecation
   window is acceptable; track in `BACKLOG.md`).

Status: **todo**.

---

## Milestone M4 — Snapshot completion for Swift-only preserved state

**Done when:** the preserved-state block in
`App/Sources/Bridge/AppStateStore+KernelProjection.swift` is deleted because
nothing in that block needs preserving — every field is projected by Rust. The
remaining chapters fallback is blocked on deleting the legacy Swift
`AIChapterCompiler` call sites.

For each preserved field, the work to remove it:

| Swift field | Required Rust work |
|---|---|
| `Episode.transcriptState` | Project from the existing transcript store via a new top-level `PodcastUpdate.transcripts: [TranscriptSummary]` (preferred) or extend `EpisodeSummary` with `transcript_state` / `transcript_path`. Drop Swift's `TranscriptIngestService` ownership of the field; it stays a capability that *reports* transcript availability into the kernel. |
| `Episode.triageDecision`, `triageRationale`, `triageIsHero` | Project from the inbox/triage Rust store onto `EpisodeSummary` (already partially wired). Make sure refresh paths emit a non-empty triage so the projection is idempotent. |
| `Episode.adSegments` fallback | Already projected. Delete only the Swift-side fallback; keep the Rust projection. |
| `Episode.metadataIndexed` | Project from `podcast-knowledge`'s index state. Add a `metadata_indexed: bool` to `EpisodeSummary` and source it from the knowledge store. |
| AI-generated chapters fallback | Rust already persists/projects AI chapters via `podcast.chapters.compile`. Move Player, Episode Detail, and transcript ingest off the legacy Swift `AIChapterCompiler`, then delete the Swift merge because Rust ships the full list. |

Status: **partial** — Rust projection exists; Swift compiler deletion remains.

---

## Milestone M5 — AI scaffolds become real (priority-ordered sub-milestones)

**Done when:** Every Tier 3 row in `docs/plan/nmp-feature-parity.md` is
`Done` or has an explicit one-release deferral row in `docs/BACKLOG.md`.

Sub-milestones run in this order because each unblocks the next:

### M5.1 — Inbox triage (unblocks AI chapters, picks)

- **Done when:** `podcast.inbox.triage` runs on a background tokio task,
  pushes incremental progress via the snapshot, and never blocks the actor
  thread. The current `runtime.block_on` (mentioned in the parity doc) is
  gone.
- Key change: move the triage executor into a long-lived
  `tokio::task::spawn` driven by a channel; emit
  `InboxSnapshot.progress` updates so the UI can show streaming results.
- File: `apps/nmp-app-podcast/src/ffi/actions/inbox_module.rs` +
  matching executor crate.

### M5.2 — Transcripts (unblocks RAG, wiki, AI chapters, agent context)

- **Done when:** Multi-source transcript discovery and fetch are kernel
  -owned. Swift exposes only an STT capability for offline fallback.
- Replace the `TranscriptIngestService` ownership of state with a
  capability-reports-only model.

### M5.3 — RAG / `podcast-knowledge`

- **Done when:** The substring ranker is replaced by the
  `podcast-knowledge` embeddings + BM25 path; indexing runs on a
  background task off feed-refresh and transcript-ready signals;
  search ops return result provenance.

### M5.4 — Agent chat

- **Done when:** Canned assistant response is gone; agent runs a real LLM
  loop with tool execution, streaming, cancellation, and memory wiring.

### M5.5 — AI chapters

- **Done when:** The equal-length stub is replaced by transcript-grounded
  chapters with persistence and regeneration. Emit through the existing
  `ChapterSummary.is_ai_generated = true` channel.

### M5.6 — Picks, TTS, wiki, voice mode

- Each gets its own sub-milestone with a one-sentence exit criterion
  consistent with the parity matrix. Do not start any of these until
  M5.1–M5.3 are done — they all depend on transcripts and/or RAG.

Status (all): **todo**.

---

## Milestone M6 — Identity and Nostr key persistence

**Status: cancelled/superseded.** The Keychain flip was explicitly cancelled:
`podcast-keys.json` is the canonical per-podcast NIP-F4 secret store, with no
Keychain migration. Identity follow-up work now lives under
`identity-kernel-actions`, `relay-list-ownership`, and the Tier 2 rows in
`docs/plan/nmp-feature-parity.md`; do not resurrect the old
`PcstIdentityCapability` keyring-slot migration plan.

---

## Milestone M7 — Compat layer burn-down

**Status: done/obsolete.** The parked `ios/` tree is absent on current `main`;
there is no `ios/Podcast/Podcast/Compat/` directory to burn down. Remaining
compatibility debt is no longer a directory milestone: it is the specific
Swift policy/fallback code still listed in `docs/BACKLOG.md` and
`docs/plan/nmp-feature-parity.md`.

---

## Milestone M8 — Runtime path consolidation + `App/Sources/` cleanup

**Done when:**

- The parked `ios/` shell remains absent.
- The 7 exit criteria at the bottom of
  `docs/plan/nmp-feature-parity.md` all pass.
- `cargo test --workspace` and the full Swift merge gate are green.

Subtasks:

1. Audit `App/Sources/` against the feature-parity matrix. Anything still
   implemented as Swift policy/fallback code comes back as a small PR per area
   before `App/Sources/` can be deleted.
2. Update `Project.swift` only if the active Tuist target shape changes.
3. Flip `docs/plan/nmp-feature-parity.md` to `Done`.
4. Delete `docs/plan/migration-v2.md` once `docs/plan.md` records
   the migration as complete (or, preferably, mark it `Archived` to
   preserve the audit trail).

Status: **todo**.

---

## Cross-cutting reminders

- **No changelog entry** for internal-only refactors. Add a one-entry JSON file
  under `App/Resources/changelog/` whenever a user-visible behaviour changes
  (e.g. "Sleep timer label now updates from Rust" produces no entry;
  "Auto-skip-ads now works on CarPlay" does).
- **Files > 300 lines** during any of these milestones need to be split
  before adding code, per `AGENTS.md`.
- **Every PR** runs `git diff --check` and the focused tests touched by
  the change. The merge gate gets the full simulator run.
- **`WIP.md`** must show every active branch + worktree before work
  starts and be cleaned up after merge.

## Order of operations (one-line summary)

M0 lock → M1 PlaybackState becomes pure → M2 single download owner → M3
settings/credentials in Rust → M4 delete preserved-state block → M5 AI
scaffolds become real (triage → transcripts → RAG → agent → chapters →
rest) → M6 identity/Nostr follow-ups without Keychain migration → M7 parked
Compat dir already absent → M8 feature parity exits pass and `App/Sources/`
becomes deletable.
