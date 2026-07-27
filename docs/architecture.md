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
model usage, generated audio provenance, tracked NMP publication receipts, and
feed-discovery download/notification policy with durable recovery. The facade contract is now version 51. It exposes bounded commands,
projections, domain events, and correlated host requests across those migrated
domains. Exact integer milliseconds, stable identifiers, explicit revisions,
effect fences, cancellation, and typed failure states prevent native adapters
from becoming a second policy or persistence owner.

Cancellable native host adapters execute URLSession/provider primitives,
AVFoundation playback, Keychain/security prompts, platform files,
notifications, speech, and other Apple capabilities. Swift renders Rust
projections and retains durable authority only for explicitly unmigrated
settings and categories, plus temporary development migration inputs. Pod0-specific Nostr
publication semantics and receipts are Rust-owned over the exactly pinned
generic NMP dependency. There is no Android product project; Kotlin binding
smoke tests and Android-compatible Rust builds are readiness checks only.

### Application state

`AppStateStore` is the `@MainActor @Observable` owner for unmigrated Swift
domains and a projection adapter for migrated slices. Views and native adapters
call typed methods; migrated commands dispatch to the shared facade, and direct
`mutateState` calls outside `App/Sources/State` are rejected by tests.

`AppState` contains replaceable projections for podcasts, subscriptions,
episodes, notes, clips, memories, and scheduled tasks. Those projections are
not written back as native durable authority. Swift remains authoritative for
settings and categories/category settings, plus explicitly retained
development migration inputs. The former Agent activity log has no live native
writer or UI; its decode-only payload is removed after Rust conversation and
memory authority are verified and is then excluded from every native write.

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

Rust owns desired state, occurrence identity, workflow stages, fences,
retry/block policy, external-operation evidence, artifact adoption, and
restart recovery for every active durable workflow. This includes publisher
and model chapters, downloads, transcript/evidence preparation, feed discovery
and notification obligations, scheduled and interactive agent work, generated
artifacts, and Pod0 Nostr publication receipts.

The Swift `WorkflowRuntime` is now an opportunity adapter only. It announces
foreground/BGTask and input changes to the Rust facade; typed native hosts
execute bounded URLSession, notification, model-provider, signer, and other
platform capabilities and return correlated observations. Swift has no active
job executor or coordinator. Residual one-shot readers for other development
cutovers are temporary cleanup debt, not a released-installation commitment.

CI runs the full Rust workspace plus named restart tests spanning chapter,
download, transcript, feed-discovery, scheduled-agent, native-effect fencing,
and publication-receipt seams. The architecture ratchet rejects any restored
native coordinator, executor, metadata-index admission, or JobStore attachment.

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
- Version 25 activates that workflow in production iOS. Rust owns restartable
  model workflow authority, including successful receipts and interrupted,
  uncertain, or terminal state, without reposting paid work. The former Swift
  planner, executor, verifier, and receipt writer are not authoritative or
  executable.
- Version 26 removed both chapter kinds from the mutable Swift job model.
  Issue #114 then deleted the development-only Swift decoder, manifests,
  compare-delete mutation, backup locations, bootstrap stages, and retirement
  marker schema. The current tree cannot decode or restore those raw job kinds;
  UI status and actions use bounded Rust publisher/model projections.
- Versions 27–46 extend the same typed, single-writer pattern through download
  workflows, recall configuration/indexing/retrieval, scheduled agents,
  interactive conversations and permissions, model-usage evidence, generated
  audio provenance, and tracked NMP publication. Version 45 also makes the
  product-proof agent catalog and provider-neutral tool definitions Rust-owned;
  native code only encodes them for the selected model provider. Swift retains
  only bounded projections and exact native capability executors for those
  migrated domains. Version 46 adds exact feed-discovery occurrences,
  Rust-owned notification eligibility/retry policy, and typed native delivery
  requests without moving UserNotifications behavior into the shared core.
- Version 47 imports every exact legacy Swift feed-discovery and notification
  occurrence through an immutable, content-qualified backup; interrupted
  notification delivery becomes terminal ambiguous evidence rather than an
  unsafe replay. Rust stages the source inertly, Swift compare-deletes only the
  verified rows and artifacts, and Rust atomically activates the imported
  workflows and notification setting. The native scheduler, reconciler,
  payloads, executors, settings persistence, and UI job projection can no
  longer create or mutate feed-discovery authority.
- Version 51 removes the model-chapter legacy cutover contract completely:
  no facade cutover methods or types, no storage authority gate, and no
  generated Swift/Kotlin bridge API remain. Model workflows operate directly
  from the Rust store.
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
