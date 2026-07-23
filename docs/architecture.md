# Pod0 architecture

This page describes the implementation on `master` and the accepted migration
direction. Detailed invariants live in the [architecture decision
index](architecture/README.md).

## Current implementation

Pod0 is a Swift 6/Tuist iOS+iPadOS application with a widget. `rust/` is an
additive Pod0-owned domain/application/facade workspace with a typed UniFFI
surface, deterministic policy tests, and an exact generic NMP pin. Generated
Swift and Kotlin APIs derive from the same Rust metadata. The Swift API is
linked into iOS as `Pod0Core` and has a runtime smoke test; the Kotlin API has a
JVM compile/runtime smoke test. `pod0-storage` provides versioned,
transactional core-schema migrations, verified backup/restore-to-staging, a
restart journal, typed read-only failure states, and verified staged imports of
the legacy Swift listening library, notes, clips, and selected transcripts.
The Rust store is authoritative for listening/library/playback; transcripts,
chapters, notes, and clips; download desired state and recovery; recall
configuration, indexing, and retrieval; publisher and model chapter workflows;
scheduled-agent definitions, occurrences, and artifacts; interactive
product-proof agent conversations, proposals, permissions, recall citations,
model usage, generated audio provenance, and tracked NMP publication receipts.
The facade contract is now version 44. It exposes bounded commands,
projections, domain events, and correlated host requests across those migrated
domains. Exact integer milliseconds, stable identifiers, explicit revisions,
effect fences, cancellation, and typed failure states prevent native adapters
from becoming a second policy or persistence owner.

Cancellable native host adapters execute URLSession/provider primitives,
AVFoundation playback, Keychain/security prompts, platform files,
notifications, speech, and other Apple capabilities. Swift renders Rust
projections and retains durable authority only for explicitly unmigrated
categories, threading records, agent activity/local diagnostic adjuncts, and
supported rollback evidence. Pod0-specific Nostr publication semantics and
receipts are Rust-owned over the exactly pinned generic NMP dependency. There
is no Android product project; Kotlin binding smoke tests and Android-compatible
Rust builds are readiness checks only.

### Application state

`AppStateStore` is the `@MainActor @Observable` owner for unmigrated Swift
domains and a projection adapter for migrated slices. Views and native adapters
call typed methods; migrated commands dispatch to the shared facade, and direct
`mutateState` calls outside `App/Sources/State` are rejected by tests.

`AppState` contains replaceable projections for podcasts, subscriptions,
episodes, notes, clips, memories, and scheduled tasks. Those projections are
not written back as native durable authority. Swift remains authoritative for
settings, categories/category settings, threading records, agent activity and
local diagnostic adjuncts, plus explicitly retained compatibility evidence.

### Persistence topology

`pod0-core.sqlite` is authoritative for migrated library/listening, playback,
notes, clips, transcripts, chapters, downloads, recall, scheduled-agent,
interactive-agent, generated-artifact, and publication-receipt state.
`Persistence` remains SQLite-authoritative for unmigrated and adjunct Swift
state. Normal reads and writes do not compare a JSON store.

- `persistence_metadata` stores a JSON-encoded `AppState` metadata snapshot
  stripped of every migrated projection after its verified authority marker,
  plus a monotonic generation.
- Legacy native episode rows are migration evidence only and are cleared by the
  verified listening cutover; they are never a concurrent live authority.
- Workflow schema metadata, jobs, and artifact records share the authoritative
  SQLite transaction boundary where atomic state/job creation is required.
- Download, staged workflow artifact, and vector-index files are derived or
  independently versioned artifacts under application support. Legacy full
  transcript JSON is read only during verified one-time migration and retained
  in an immutable backup; normal reads and writes use Rust-owned canonical
  transcript artifacts and selections.
- Legacy JSON is imported once and is never a concurrent authority.
- Keychain stores provider secrets. iCloud KVS carries selected non-secret
  settings. The widget reads a bounded app-group snapshot.

Swift state writes use monotonic revisions and a serialized background writer.
Projection updates never trigger native persistence, iCloud, widget, or
indexing side effects. A verified cutover performs one explicit cleanup write.
Shared playback observations are coalesced to one second and Rust commits the
first position, semantic boundaries, and a maximum 30-second cadence.

### Durable workflows

The Swift workflow runtime currently provides these facilities for domains
that have not yet migrated:

- deterministic desired-job planning;
- idempotency keys and occurrence identity;
- SQLite job state, leases, fencing tokens, attempt/retry/block state;
- external-operation evidence to avoid duplicate provider charges;
- staged artifact verification and atomic adoption;
- BGTask opportunities and background URLSession reconciliation;
- restartable schema migrations and a process-reconstruction harness.

Publisher chapter acquisition no longer uses this runtime: Rust owns its
desired state and lifecycle, while Swift performs only the exact bounded GET
and renders the Rust projection. The remaining Swift runtime is a
characterization baseline for later vertical-slice migrations.

### Presentation and platform capabilities

SwiftUI owns rendering, native navigation/transitions, accessibility, animation,
and transient presentation state. Swift also owns AVFoundation, audio sessions
and routes, media controls, BGTask/URLSession entry points, notifications,
Keychain/biometric prompts, widgets, Spotlight, file/share integration, and
Apple speech/audio capture.

The feed and playback adapters now execute typed host requests and return raw,
deadline/cancellation-safe observations. Other native components adopt the same
boundary as their domains migrate. They do not become a second durable policy
owner.

## Target ownership

The [machine-readable ownership inventory](architecture/ownership.json)
classifies every production Swift file. Its checker fails on uncovered or
ambiguous production code.

The Pod0 Rust kernel progressively owns:

- stable product identities, schemas, and migrations;
- subscription/feed normalization and durable library state;
- queue, resume, completion, playback-rate, and sleep-timer policy;
- transcript normalization, chapters, semantic spans, provenance, and search;
- highlights, notes, clips, conversations, briefings, and artifacts;
- download/workflow desired state, retry, cancellation, and recovery;
- agent validation, permission, commit, and generated-artifact semantics;
- Pod0-specific Nostr behavior over a pinned generic NMP dependency.

## Native/shared communication

There is one app-owned facade contract with committed, reproducibly generated
Swift and Kotlin bindings. CI rejects drift from Rust metadata.

- Native dispatches typed fire-and-forget commands.
- One Rust actor is the writer for migrated state.
- Async/native results return as typed internal events or host observations.
- Feed hosts return bounded bytes, validators, redirect URL, and HTTP evidence;
  Swift does not normalize the payload on the shared path.
- Playback hosts execute AVFoundation primitives and coalesce lifecycle
  observations; queue/resume/completion decisions never enter the adapter.
- Transcript contract qualification is a pure, bounded, state-shaped
  preflight; invalid input becomes rejected projection state.
  Legacy Swift `TimeInterval` transcript bounds cross this boundary exactly
  once: reject non-finite, negative, or overflowing values, multiply seconds by
  1,000, then round to the nearest whole millisecond with ties away from zero.
  Only the resulting integer milliseconds may be persisted or fingerprinted.
  Version 13 commits accepted transcript observations through the application command and
  reads them back through bounded summary/speaker/segment/word projections.
  Swift maps native/provider observations into this command and reconstructs
  presentation values from these projections; it owns neither the selection
  nor a durable transcript copy.
- Version 13 introduced the canonical chapter/ad-span
  contract: stable artifact/item IDs, integer-millisecond bounds, source and
  transcript provenance, explicit not-evaluated versus evaluated-empty ad
  state, deterministic inferred ends, and bounded summary/item projections.
  The #99–#104 cutover made the Rust store the sole production chapter writer
  and deleted the former Swift authority.
- Version 17 evaluates next/previous targets and half-open, one-time ad skips
  from the Rust-selected immutable artifact. The bounded playback projection
  carries its artifact/revision/session fence, and the native audio host
  executes an exact typed seek against the active Rust chapter authority.
- Versions 20–21 add the first Rust-owned durable publisher-chapter workflow. Rust derives
  publisher intent from feed metadata, persists stable request/cancellation
  identity and absolute retry time, classifies raw HTTP facts, qualifies and
  commits the artifact atomically, adopts current legacy selections, and
  exposes bounded status/actions. Rust admission and native execution are both
  bounded; source replacement produces an exact typed cancellation, and an
  accepted observation remains recoverable until its SQLite transition
  commits. Swift contains no publisher scheduler, retry policy, receipt,
  verifier, or writer.
- Version 22 moved generated/enriched chapter-model request policy into Rust.
  The facade reads the authoritative episode, selected transcript, and selected
  chapter directly; it returns one typed, bounded request containing the exact
  provider/model, prompt contract, response format, provenance expectation,
  input version, and chapter-selection fence. Swift executes that request and
  returns raw provider evidence. It no longer constructs prompts, selects the
  generation/enrichment mode, parses model settings, or versions model inputs.
- Version 24 completes the typed durable chapter-model workflow surface. Rust
  owns claim-before-delivery, a single active model operation, submission
  fences, provider-operation recovery, retry/backoff decisions, raw completion
  staging, qualification, provenance, atomic artifact commit, and bounded
  workflow projections. Paid completion evidence is discarded only after a
  typed durable receipt. A typed core-wake request makes delayed retries and
  staged-completion recovery event-driven without native polling. Swift and
  Kotlin receive only the minimum provider execution/recovery contract; secrets
  remain native.
- Version 25 activates that workflow in production iOS. A typed, restartable
  cutover adopts exact current successful legacy receipts and reconstructable
  interrupted, uncertain, or terminal state without reposting paid work. Stale
  or unplannable rows remain rollback evidence rather than becoming a second
  Rust workflow format. Before deletion it durably writes a content-qualified,
  integrity-checked classification manifest containing every legacy job row. The no-clobber
  manifest is retained beside the episode store under the
  `model-chapter-workflow-backups` suffix through the documented rollback
  support window. Only after the manifest is re-read and verified may the
  cutover delete legacy rows and commit the Rust authority marker; staged
  restarts verify the exact source. A changed, still-present legacy source
  discards only the generation-fenced Rust stage and restages from the new
  snapshot; missing rows without the verified backup fail closed.
  The former Swift planner, executor, verifier, and receipt writer are no longer
  authoritative or executable.
- Version 26 removes both chapter kinds from the mutable Swift job model. A
  quarantined compatibility decoder preserves every pre-cutover model and
  publisher row in immutable, integrity-checked manifests; it never feeds the
  native scheduler or UI. After Rust model authority is verified, one immediate
  SQLite transaction proves model rows absent, compare-deletes the exact
  publisher source, and commits `legacy_chapter_workflow_retirement`. Generic
  native recovery and claim SQL accepts only current `WorkJobKind` values, while
  UI status and actions use read-only Rust publisher/model projections. The
  compatibility bridge remains only for supported direct upgrades and is
  deleted under issue #114 after the two-release/90-day support gate.
- Versions 27–44 extend the same typed, single-writer pattern through download
  workflows, recall configuration/indexing/retrieval, scheduled agents,
  interactive conversations and permissions, model-usage evidence, generated
  audio provenance, and tracked NMP publication. Swift retains only bounded
  projections and exact native capability executors for those migrated domains.
- Open views receive bounded, revisioned, screen-shaped projections.
- Operation failure and cancellation appear in projection state, not thrown
  per-operation FFI results.
- Subscriptions are explicit and event-driven; polling is forbidden.
- High-frequency playhead animation stays native. Only bounded observations
  needed for durable policy cross FFI.

See [ADR-0003](architecture/adr/0003-typed-uniffi-application-facade.md).

## Migration sequence

1. Architecture rules, ownership inventory, and CI ratchets.
2. iOS listening-to-recall product proof in parallel.
3. Rust workspace, schemas, typed facade, Swift/Kotlin generation, and
   Apple/Android compile checks.
4. Subscribe → library → episode detail → native play → durable resume as the
   first complete Rust-authoritative slice.
5. Transcript/knowledge vertical slices.
6. Download/workflow/agent/Nostr vertical slices.
7. Evidence-based Android investment gate; Android product work only after go.

Every cutover uses one writer, preserves existing data, verifies migration and
restart behavior, and deletes replaced ownership immediately. The executable
dependency graph is in the [roadmap](../Plans/2026-07-18-ios-first-rust-nmp-roadmap.md).

## Enforcement

- `scripts/check_architecture_docs.py` rejects facade-version drift, duplicate
  ownership keys, stale current-authority claims, and unmarked historical plans.
- `scripts/check_architecture_ownership.py` covers every production Swift file.
- `scripts/check_ui_storage_boundary.py` rejects new presentation-to-repository
  access and tracks exact temporary exceptions with deletion issues.
- `scripts/check_transcript_single_writer.py` rejects any reintroduced Swift
  transcript store, shadow path, readiness mutator, or workflow artifact writer
  and requires the typed Rust commit/read/migration seams.
- `AppStateMutationBoundaryTests` rejects direct production `mutateState` use
  outside the State domain.
- The pull-request template requires an ownership declaration for
  cross-platform-sensitive work.
- CI and AGENTS.md enforce architecture, typography, changelog, and line-limit
  rules as their ratchets land.
