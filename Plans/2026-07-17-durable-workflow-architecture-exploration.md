# Pod0 durable workflow architecture exploration

Date: 2026-07-17
Project/context: pod0 (Podcastr iOS app), async workflow durability / recovery-after-interruption
Status: archived (promoted to GitHub epic #19)

## Core Question

- The app's product workflows (download → transcribe → index → chapters/wiki; feed refresh → dispatch; scheduled agent runs) are chains of imperative callbacks and untracked `Task`s. What architecture should replace this so the system converges to required outcomes after interruption, and what is the migration path?

## Current Working Model

- Root cause: the app was bootstrapped from a template whose architecture (`docs/architecture.md`) is single `AppState` + save-on-mutation with explicitly "no effects system." Six async pipelines were added without ever adding an orchestration layer; state records only completed facts, never owed work.
- Proposed direction (not yet user-approved): durable convergent job layer —
  1. `JobStore`: jobs table in the existing SQLite sidecar `(id, kind, subjectID, state, attempts, notBefore, createdAt)`, written transactionally with the spawning mutation where possible.
  2. `WorkCoordinator` actor: sole executor; per-kind `JobExecutor`s; bounded concurrency; backoff. Downloads stay on background `URLSession` (already durable) — coordinator only observes completion.
  3. `Reconciler`: launch/foreground sweep deriving owed jobs from observable state (`downloaded && !transcriptReady → ingest`, etc.); resets orphaned in-progress states. Converge-don't-remember: journal loss self-heals.
  4. One flag per outcome: split `transcriptState` from chunk-index state; un-overload `metadataIndexed`; per-artifact `none/pending/ready/failed(attempts)` for chapters, ad segments, wiki, notifications.
  5. Consume-on-success with lease for scheduled runs (the `InboxTriageService` pattern).
  6. Guarded store: `private(set) state`, validated transitions, store-driven projection invalidation.
  7. `BGTaskScheduler` (app-refresh + processing) — today there is zero background execution beyond download events.
- In-codebase exemplars proving the pattern fits: `EpisodeMetadataIndexer` (persisted flag + launch backfill = convergent) and `InboxTriageService` (write-on-success = natural retry).

## Observations

- Five Opus audit agents (2026-07-17) confirmed 7 of 8 claims from an external review; full reports in this session's transcript. Highlights:
  - Download completion persists `.downloaded` then fires unawaited ingest `Task` (`EpisodeDownloadService+Delegate.swift:258-284`). Mid-ingest interruption leaves persisted `.transcribing`/`.fetchingPublisher` that BOTH recovery surfaces refuse (`EpisodeDetailView.swift:117` guards `.none`; `TranscribingInProgressView.swift:136-142` disables manual retry) → permanently stuck, no recourse. Background-download relaunch is the most likely path to strand episodes (iOS suspends after download events drain, killing chained ingest).
  - `TranscriptionQueue.swift` is dead code — zero callers. Real mechanism is `TranscriptIngestService` with in-memory `inFlight` set. `ingestPending` (would-be reconciler) is never called.
  - Feed refresh: side effects (auto-download, ingest, notifications) held in in-memory `PendingSideEffects` for up to 60s behind LLM triage wait (`SubscriptionRefreshService.swift:142-161`); backgrounding cancels the holding task; keyed to `newlyInsertedIDs` so never retried. Only metadata indexing is convergent.
  - Derived content: `persistAndIndex` (`TranscriptIngestService.swift:302-401`) flips `.ready` before embedding; chunk-index failure swallowed; metadata backfill then embeds shallow title/desc chunk and flips the same overloaded `metadataIndexed` flag → failure permanently masked, search silently degraded. No re-index path exists. Chapters/wiki are detached unawaited `Task`s; wiki queue RAM-only (`WikiRefreshExecutor.swift:60-62`); `WikiTriggers`' intended durable scheduler was never built.
  - Scheduled agent runs: `AgentScheduledTaskRunner.swift:26-33` consumes slot (`markTaskRun` advances `nextRunAt`) before fire-and-forget run; no-key/LLM-error/turn-exhaustion burn interval, no retry; async persist means marker can be lost on hard kill → not even reliably at-most-once. `retry()` wired only to UI chat.
  - Persistence: `Persistence.save` background mode spawns per-call unordered `Task.detached` → older snapshot can win; writer (`PersistenceBackgroundWriter.swift:7-22`) is last-enqueue-wins, no revision gate; untested (durability tests use `.immediate` only). SQLite sidecar + JSON metadata written non-atomically w.r.t. each other, skew undetected at load. Background flush is fire-and-forget, no `beginBackgroundTask`/`willTerminate` hook.
  - Store: all setters accept any transition; `store.state` publicly mutable and externally written (`NostrAgentResponder.swift:142`); projection cache correctness depends on callers manually calling `invalidateEpisodeProjections()` (shallow fingerprint `AppStateStore+EpisodeProjections.swift:245-251`).
  - No `BGTaskScheduler`/`BGAppRefreshTask` anywhere (grep-confirmed).
- Corrections to the external review: download queue is durable (bg `URLSession` + `taskDescription` + persisted `.queued`) — that half refuted; "transcription queue" reasoning based on dead code; inbox triage/agent picks already use the correct write-on-success pattern.

## Constraints And Invariants

- iOS process model: app can be suspended/killed at any await point; background execution only via bg URLSession events and (if added) BGTaskScheduler with short time budgets.
- `AGENTS.md`: 300-line soft / 500-line hard file limit; whats-new.json entry for user-facing changes; no serif fonts (irrelevant here).
- Existing durable substrate to build on: SQLite episode sidecar (WAL) + atomic JSON metadata file; App Group container shared with widget.
- Downloads pipeline must not regress — it is the one already-durable pipeline.

## Preferences

- (from external review, endorsed by user's framing) Durable, declarative workflow architecture: explicit desired outcomes, observable artifact state, recoverable orchestration; converge after interruption rather than relying on first-time callback success.
- User wants a "cleaner architecture we can build on" and an epic + issues to get there.

## Assumptions

- Hand-rolled job layer preferred over adopting a dependency (GRDB job queue, event-sourcing framework) — inferred from the codebase's zero-dependency style; verify with user.
- Filing the epic/issues on GitHub (`pablof7z/pod0`) is the intended durable artifact form — user has a `/todo` skill that files GH issues; awaiting explicit go-ahead.

## Open Questions

- Does the user approve the (revised, post-narrowing) direction: minimal JobStore for non-derivable work only + WorkCoordinator + Reconciler? Alternative still on the table: pure reconciler, attempts/backoff tracked on episode fields.
- Notification semantics: is a late (reconciled) new-episode notification acceptable, or should missed notifications be dropped rather than delivered stale?
- Where should the epic/issues live (GitHub issues vs Plans/ docs)?

## Resolved By Surface Narrowing (2026-07-17, see [[2026-07-17-product-surface-narrowing-exploration]])

- Triage deleted → the 60s dispatch window, triage-gating question, and `waitForTriageToSettle` are gone; feed refresh now dispatches side effects immediately post-upsert (implemented on `surface-narrowing` branch).
- Wiki + briefings deleted → wiki RAM-only queue, WikiTriggers scheduler gap, briefing pipelines all out of scope; derived artifacts shrink to: transcript, chunk index, chapters/ad-segments, notifications.
- Dead `TranscriptionQueue` deleted (was epic issue 5). Feedback + nostr deleted → `NostrAgentResponder`'s direct `store.state` mutation (invariant-violation example) gone.
- Epic shrinks from 17 issues to ~12; scheduled-agent-run durability gains importance (proactivity is a headline keep + needs BGTaskScheduler).

## Hypotheses

- A pure reconciler (derive all owed work from state, no job journal) could cover most pipelines; the journal mainly adds attempts/backoff/notBefore bookkeeping and work not derivable from state (notifications, scheduled runs). Unproven — may simplify Phase 1.
- The 60s triage-settle coupling can be removed without product regression by letting triage archive episodes after side effects are recorded (jobs cancelable on archive). Needs product confirmation.

## Risks

- Migration risk: pipelines migrate one at a time; interim period has two dispatch mechanisms coexisting — reconciler must not double-fire work already dispatched the old way (idempotency keys per (kind, subjectID) mitigate).
- Data already in the wild: episodes stuck in `.transcribing`, silently-unindexed transcripts (masked by `metadataIndexed`) — need one-time repair migrations, not just new-path correctness.
- Retry storms: reconciler + backoff must cap attempts or a persistently-failing LLM/API job could burn battery/quota.
- `private(set) state` refactor touches preview/fixture code and `NostrAgentResponder` — mechanical but wide.

## Evidence Gathered

- Five audit reports (probe-downloads, probe-feeds, probe-derived, probe-persistence, probe-agents), 2026-07-17, all with file:line anchors — summarized under Observations; full text in session transcript.
- Firsthand reads: `PersistenceBackgroundWriter.swift`, `TranscriptionQueue.swift`, `Persistence.swift`, `docs/architecture.md`.

## Adjacent Checks

- (none yet beyond the five primary audits)

## Alternatives Considered

- Event sourcing / full workflow framework (Temporal-style, TCA effects): rejected — heavy rewrite, poor fit for iOS process model, codebase's own exemplars show the convergent-flag pattern suffices.
- Status quo + spot fixes only: rejected — every new pipeline re-inherits the same fragility; the review's list would regrow.
- Pure reconciler without job journal: still open as a simplification (see Hypotheses).

## Rejected Options

- Treating `TranscriptionQueue` as the thing to "make durable": it is dead code; delete instead.
- Making downloads part of the new job layer's execution: bg URLSession already durable; coordinator observes only.

## Decisions Or Emerging Direction

- DECIDED (user, 2026-07-17 PM — "go for it"): durable convergent job layer = **minimal JobStore + WorkCoordinator + Reconciler** (my recommendation over the pure-reconciler variant). Journal holds ONLY non-derivable work (attempt counts/backoff/notBefore, scheduled-run leases, notification-sent markers); everything derivable from observable episode state stays journal-free and is re-derived by the Reconciler each launch/foreground. Plus: per-artifact state flags, validated store transitions, persistence revision ordering, BGTaskScheduler. Pure-reconciler variant → Rejected (smears retry bookkeeping across domain models).
- DECIDED: missed new-episode notifications older than ~24h are DROPPED by the reconciler, not delivered stale; all other owed work always converges.
- Epic filed on GitHub after surface narrowing merged (PR #18). See Follow-Up Artifacts for issue numbers.

## Follow-Up Artifacts

- Surface narrowing PR #18 — MERGED to master 2026-07-17 (squash `04953483`), −31,102 net LOC.
- GitHub epic **#19** filed 2026-07-17 with 14 issues:
  - Phase 0 (safety): #20 persistence ordering, #21 suspend flush + generation stamp, #22 unstick transcript states, #23 scheduled-task lease.
  - Phase 1 (substrate): #24 JobStore, #25 WorkCoordinator, #26 Reconciler.
  - Phase 2 (migrate): #27 feed-refresh side effects, #28 transcript-index split + repair, #29 chapters/ad-segments, #30 scheduled runs → coordinator.
  - Phase 3 (harden): #31 guarded store, #32 BGTaskScheduler, #33 interruption test suite.
- Next actionable: Phase 0 issues (#20-#23) are independent and can start immediately.
