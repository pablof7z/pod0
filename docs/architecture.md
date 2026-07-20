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
the legacy Swift listening library, notes, clips, and selected transcripts. The
Rust store is authoritative for podcasts, subscriptions, episode listening
facts, active playback, queue, resume, completion, rate, playback preferences,
session sleep mode, notes, saved clips with immutable transcript provenance,
and selected canonical transcripts.
The facade contract is now version 12 and includes an additive canonical
transcript-artifact contract: exact integer milliseconds, full word and speaker
records, deterministic semantic/version/artifact identities, unknown-source
preservation, replay fingerprints, and separately bounded summary, speaker,
segment, and word projections. Its pure contract projection represents invalid
input as rejection state rather than an FFI exception. At startup the iOS shell
verifies and stages the issue #95 legacy transcript import, commits selected
artifacts and the authority marker in one Rust transaction, and only then opens
the application facade. The version-12 facade accepts typed native transcript
observations and exposes bounded transcript projections. The issue #97 cutover
removed Swift transcript persistence, readiness mutation, workflow artifact
ownership, and the temporary shadow path.
Cancellable native host adapters now
execute typed feed requests through URLSession and playback requests through
AVFoundation, returning correlated bounded observations through the generated
contract. Swift renders shared library/playback projections and retains adjunct
state only for domains that have not migrated. There is no Android product
project. The NMP adapter remains isolated from the facade while the security
hold in issue #85 is active.

### Application state

`AppStateStore` is the `@MainActor @Observable` owner for unmigrated Swift
domains and a projection adapter for the migrated listening, notes, and clips slices. Views and
agent adapters call typed methods; migrated library/playback methods dispatch
to the shared facade, and direct `mutateState` calls outside
`App/Sources/State` are rejected by tests.

`AppState` currently contains replaceable podcast, episode, note, and clip projections plus
settings, agent memory/activity, categories, threading records, scheduled tasks,
and the last-played episode. This is migration input, not the final
cross-platform schema.

### Persistence topology

`pod0-core.sqlite` is authoritative for the migrated listening, notes, clips,
and selected-transcript slices.
`Persistence` remains SQLite-authoritative for unmigrated and adjunct Swift
state. Normal reads and writes do not compare a JSON store.

- `persistence_metadata` stores a JSON-encoded `AppState` metadata snapshot
  without migrated episode, note, or clip authority plus a monotonic generation.
- `episodes` stores one versioned JSON payload per episode with stable local ID
  and sort order.
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
Shared playback observations are coalesced to one second and Rust commits the
first position, semantic boundaries, and a maximum 30-second cadence without
rewriting the Swift metadata snapshot.

### Durable workflows

The Swift workflow runtime currently provides:

- deterministic desired-job planning;
- idempotency keys and occurrence identity;
- SQLite job state, leases, fencing tokens, attempt/retry/block state;
- external-operation evidence to avoid duplicate provider charges;
- staged artifact verification and atomic adoption;
- BGTask opportunities and background URLSession reconciliation;
- restartable schema migrations and a process-reconstruction harness.

This implementation is a characterization baseline for the later Rust workflow
migration. It is not disposable scaffolding.

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
- Version 13 also defines the non-persisting canonical chapter/ad-span
  contract: stable artifact/item IDs, integer-millisecond bounds, source and
  transcript provenance, explicit not-evaluated versus evaluated-empty ad
  state, deterministic inferred ends, and bounded summary/item projections.
  Swift remains the single production chapter writer until the staged
  migration and atomic authority cutover in #99–#104; the contract is not a
  shadow durable store.
- Version 17 evaluates next/previous targets and half-open, one-time ad skips
  from the Rust-selected immutable artifact. The bounded playback projection
  carries its artifact/revision/session fence, and the native audio host
  executes an exact typed seek. The policy remains dormant in production until
  #104 atomically activates chapter authority, switches callers, and deletes
  the temporary Swift rules.
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
