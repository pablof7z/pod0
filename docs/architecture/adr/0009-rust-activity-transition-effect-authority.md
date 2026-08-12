# ADR-0009: Rust activity, transition, and effect authority

- Status: Accepted
- Date: 2026-08-12
- Decision owners: Pod0 application architecture
- Related issues: #204, #205, #206, #207, #208, #209, #210, #211, #212, #219
- Supersedes: ADR-0001's temporary-Swift business-logic allowance and any earlier
  text that permits native product policy after this decision

## Context

Pod0 already routes typed commands and host observations through Rust, and many
workflow domains have typed stages, fences, restart recovery, and durable
SQLite state. Those pieces do not establish a universal activity invariant.

Current durable evidence is fragmented among command receipts, mutable workflow
rows, attempt tables, agent-specific audit rows, and in-memory operation state.
Episode Diagnostics reads a separate capped and clearable Swift JSON store.
Its callers choose whether to record an entry, so automatic, playback-triggered,
agent-triggered, recovery, and future paths can be absent.

A request log alone is insufficient. Logging before admission can claim work
that Rust rejected. Logging after mutation or effect execution leaves a crash
window in which real activity has no durable evidence. A central service that
also decides every domain's transitions would solve neither problem cleanly; it
would become a policy god object.

## Decision

All Pod0 product and business logic is Rust-owned.

Every product input enters an explicitly owned typed Rust state machine or
transition function. The owner decides legality and returns a pure transition
plan. A small policy-free Rust transition committer is the only authority that
may persist an authoritative product-state mutation. It atomically persists:

1. the current-state mutation;
2. a non-empty set of immutable semantic activity facts when state changes;
3. durable external-effect and internal-command intents authorized by facts;
4. the idempotency receipt and typed request disposition.

Native code renders Rust projections or executes exact typed platform
capabilities claimed from the Rust outbox. It cannot decide whether work is
allowed, which work is next, whether to retry or fall back, or what durable
outcome an observation means.

This is not full event sourcing. Current-state tables remain authoritative for
operational state. The immutable activity journal is historical and causal
truth committed beside those tables.

## Terms

- **Input**: a command, host observation, internal command, wake, timer,
  migration action, or recovery trigger presented to a Rust domain owner.
- **Request disposition**: the durable outcome of admission, including
  accepted, rejected, stale, duplicate, not allowed, already complete, or no-op.
- **Transition plan**: a typed, pure result containing expected revisions,
  domain mutations, facts, and authorized work intents.
- **Activity fact**: an immutable semantic statement about a request,
  transition, authorization, or outcome.
- **External-effect intent**: durable authorization for one bounded native or
  provider capability.
- **Internal-command intent**: durable causally linked work submitted to another
  Rust domain through its normal ingress.
- **Transport evidence**: delivery/staging evidence such as the native
  observation outbox. It is not product truth.
- **Diagnostic projection**: a bounded Rust read model derived from canonical
  facts. It is not a writer.

## Single-responsibility boundaries

### Typed Rust domain state machine

Owns transition legality, admission, validation, authorization, sequencing,
retry/backoff/fallback policy, cancellation semantics, recovery, and semantic
fact selection for one domain.

It reads typed current state and one typed input and returns a transition plan
or typed rejection. It does not write SQLite, call platform APIs, or render UI.
Unrelated domains retain separate state and transition types.

Long-lived workflows use explicit states and exhaustive transition tables.
Simple CRUD-like behavior uses the same plan/commit protocol as a single
transition; it does not require an artificial generic state graph.

### Rust transition committer

Owns transaction atomicity, optimistic revision checks, append-only fact
insertion, effect/internal-command insertion, and idempotency receipts.

It does not decide domain legality, provider selection, retry policy, or UI
semantics. It accepts typed domain mutation capabilities, never arbitrary SQL
closures or generic mutation maps.

### Rust activity journal

Owns immutable causal facts, store-assigned ordering, subject association,
redaction-safe payloads, and bounded projections.

It does not schedule work, mutate current state, or replace domain tables.
Product APIs expose append/read behavior only. Schema integrity rejects update
and delete outside separately governed whole-user-data erasure.

### Rust durable outboxes

The external-effect outbox owns authorization identity, claim/lease state,
attempt identity, fencing, cancellation delivery, restart recovery, and honest
ambiguous outcomes.

The internal-command outbox carries cross-domain consequences with correlation
and causation identity. A domain cannot directly mutate another domain's store.

Neither outbox decides what effect or retry is appropriate; domain state
machines decide those transitions.

### Rust facade/router

Owns exhaustive input routing, state loading, transition invocation, commit,
and bounded projection delivery.

It does not contain domain policy, directly mutate product stores, fabricate
in-memory-only effects, or accept future variants through wildcard behavior.

### Native capability executor

Owns literal execution of a claimed platform/provider primitive and mapping of
its raw bounded result into a typed observation.

Allowed native responsibilities include SwiftUI/Compose presentation,
accessibility, localization, transient UI and OS handles, AVFoundation/Media3,
URLSession/network calls, BGTask/WorkManager entry points, notifications,
speech APIs, file/share pickers, Keychain/Keystore custody, generated-type
marshalling, transport staging, and exact capability cancellation.

Native code must not perform product admission or normalization; select a
provider/model; decide retry, timeout, fallback, recovery, or next work;
authorize a tool or effect; mutate authoritative product state; append
canonical facts; associate causality; or interpret delivery as product success.

Wire-format decoding may produce a bounded raw observation. Product
normalization, validation, confidence thresholds, adoption, and durable
meaning remain Rust-owned.

### Native presentation

Owns layout, animation, accessibility, localized copy, and transient view
state. It renders Rust projections and submits typed user intent.

It must not derive allowed actions, maintain a canonical activity cache, write
product storage, or synthesize a successful outcome.

## Ingress and disposition rules

Every submitted product command receives durable request evidence. Accepted
commands record their semantic transition. Rejected, stale, conflicting,
not-allowed, duplicate, and already-complete requests record a bounded
disposition without claiming that state changed or an effect ran.

A repeated command ID with the same fingerprint returns the original durable
disposition. Reuse with a different fingerprint fails closed.

Host observations, internal commands, wakes, and recovery triggers use the same
domain transition and commit protocol as live commands. Recovery cannot repair
state through a direct store write.

Every command, host request, host observation, recovery surface, authoritative
storage mutator, and native executor belongs in the machine-readable
conformance inventory under `docs/architecture/activity-conformance/`.

## State and fact atomicity

A transaction that changes authoritative product state must append at least one
semantic activity fact in the same SQLite transaction.

A transaction that persists an external effect or internal command must link it
to an authorizing fact. Database foreign keys enforce that link.

Rejected and true no-op inputs do not need a state-transition fact, but their
request disposition remains durable. Polls and reconciliation passes that
discover no semantic change append nothing.

No network, provider, filesystem artifact transfer, media operation, or other
external I/O occurs inside the SQLite transaction.

## Effect and observation rules

Only persisted, claimable external-effect rows may cross the native boundary.
`next_host_requests` or its successor is a projection/claim operation over
that outbox; it cannot manufacture work from an in-memory queue.

A native observation preserves effect intent, attempt, lease/fence,
cancellation, correlation, and causation identities. Rust decides whether it is
current and what transition follows.

Delivery is not completion. If the process dies after an external system may
have succeeded but before Pod0 commits the observation, the durable state is
outcome-unknown unless a provider idempotency key or retained receipt can prove
and reattach the result.

One effect attempt has one terminal durable outcome. A long-lived native or
provider stream is therefore modeled as a sequence of bounded leased
capabilities, not as several observations smuggled through one lease. For
example, accepting a download start completes that effect and atomically
authorizes a fenced attach/await-completion effect keyed by the external task;
artifact adoption remains a later Rust transition. This keeps every process
death boundary explicit and prevents a streaming callback from becoming an
unlogged second ingress.

Cross-domain consequences are persisted internal commands. For example,
playback-triggered transcript preparation enters the transcript state machine
with the playback fact as its cause.

## Activity semantics

The journal records meaningful product facts, not telemetry.

Persisted examples include request dispositions, workflow transitions,
effect authorization and terminal/ambiguous outcome, durable playback
checkpoints, artifact adoption or selection, user artifact mutations,
permission decisions, recovery transitions, and data-authority cutovers.

UI taps, rendering, scrolling, raw audio positions, byte callbacks, repeated
polls, and reconciliation no-ops are not canonical facts. A high-frequency
sample becomes a fact only when Rust commits it as an authoritative semantic
checkpoint.

Facts use typed stable discriminants and versioned bounded payloads. They carry
store sequence, activity and transaction identities, actor/origin, direct
subject, optional deliberate episode association, correlation, causation,
command/disposition identity, and recorded time. Wall-clock time is display
evidence, not the sole ordering mechanism.

## Privacy, retention, and erasure

Canonical facts may contain stable IDs, typed status/reason codes, bounded safe
metadata, hashes, and non-secret counts. They must not contain transcript or
note bodies, prompts, model responses, memories, credentials, secrets, raw
provider bodies, or arbitrary local paths.

Episode Diagnostics cannot update, delete, replace, silently cap, or clear the
canonical journal. A user-visible clear action may advance a local view
bookmark. Export, retention, and whole-user-data erasure are explicit
Rust/storage operations with their own facts and legal/privacy review.

OSLog, metrics, crash logs, and native transport staging remain separate
observability systems.

## Crash and recovery semantics

The implementation must test these boundaries:

- before commit: no state, fact, receipt, or effect is visible;
- after commit and before claim: state and authorization survive restart;
- during a lease: the effect is reclaimed or resolved under typed fence rules;
- after possible external success and before observation: outcome is unknown
  unless durable external evidence proves otherwise;
- during observation commit: outcome state and facts are atomic;
- after commit response loss: idempotent replay returns the original receipt.

No path converts uncertainty into success merely to make recovery progress.

## Migration

The kernel lands dark. Domains cut over one ownership-complete slice at a time
behind explicit authority markers.

Shadow reads may compare old and new representations. Durable dual writes and
dual effect dispatch are forbidden. Before a marker commits, rollback discards
staged new rows. After it commits, rollback requires a tested versioned
export/restore path and cannot silently reactivate Swift authority.

Every temporary native-policy or direct-writer exception is exact, linked to
one cutover issue, and non-growing. The final #219 gate requires the exception
set to be empty and every conformance row implemented and verified.

## Mechanical enforcement

CI must fail when:

- a command, request, or observation variant lacks an inventory owner;
- a recovery or native execution surface is unclassified;
- a product-state mutation can bypass the transition committer;
- an effect can be claimed without an authorizing fact;
- a router uses wildcard acceptance for a new input;
- Swift/Kotlin adds product policy or direct durable mutation;
- feature/UI code invokes a capability outside the dispatcher;
- canonical activity is fabricated outside Rust;
- append-only schema protections are missing;
- a migration exception grows or lacks a deletion issue.

Negative fixtures are required; a green scan without a demonstrated rejected
bypass is not sufficient proof.

## Consequences

The design makes bypasses expensive in the useful sense: they must cross typed
crate boundaries, database constraints, exact inventories, and negative CI
fixtures.

Domain policy remains cohesive and independently testable. The shared commit
protocol remains small. Diagnostics becomes trustworthy without turning the
entire application into an event-sourced system.

The migration is substantial. Until #219 closes, the repository must describe
coverage as partial and must not claim the universal invariant is already true.

## Rejected alternatives

- **Swift/UI call-site logging:** incomplete and not atomic with Rust state.
- **Command receipts as the journal:** insufficient semantics and observation
  coverage.
- **Mutable workflow tables as history:** fragmented and not append-only.
- **Logging after effects:** leaves an unavoidable unrecorded crash window.
- **One central activity manager with domain policy:** violates SRP.
- **One generic string-based FSM:** erases domain exhaustiveness.
- **Full event sourcing:** unnecessary operational and migration complexity.
- **Permanent native-policy exceptions:** creates the second leg this decision
  exists to prevent.
