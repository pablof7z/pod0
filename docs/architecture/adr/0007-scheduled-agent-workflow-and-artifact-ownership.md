# ADR-0007: Scheduled agent workflow and artifact ownership

- Status: Accepted
- Date: 2026-07-22
- Decision owners: Pod0 application architecture
- Related issues: #60, #125, #126, #127, #128, #129, #130

## Context

Scheduled agent work is currently one user flow with several durable Swift
owners. `AppStateStore` stores recurring definitions, `DesiredStatePlanner`
derives due occurrences from `Date`, `JobStore` owns attempts and retry state,
`ChatHistoryStore` can declare an occurrence complete, and
`ArtifactRepository` stores a second completion fact. `AgentChatSession` and
`ScheduledAgentRunJobExecutor` also combine provider execution with retry,
resume, and completion policy.

Those decisions are cross-platform and termination-sensitive. Leaving them in
Swift would require Android to reproduce occurrence identity, paid-operation
deduplication, fencing, retry, and artifact-commit behavior. Moving provider or
presentation primitives to Rust would instead weaken native product quality.

## Decision

Pod0 Rust owns scheduled task definitions, occurrence identity, lifecycle,
attempt fencing, retry and blocking decisions, cancellation, recurrence
advancement, generated-output evidence, and artifact commit state. Rust is the
only durable writer after the scheduled-agent authority marker commits.

Native iOS, and later Android, execute bounded provider, tool, notification,
security, file, and presentation capabilities. A native adapter may retain an
in-flight OS/provider handle and transient streaming text. It cannot decide
whether an occurrence is due, retryable, complete, authorized, or selected.

Generic NMP does not own Pod0 scheduled jobs or artifacts. A later Pod0 Nostr
slice may publish a committed artifact through `pod0-nmp`, but publication is a
separate durable obligation and is never evidence that the agent occurrence
completed.

## Single-writer ownership

| Fact | Before cutover | After cutover |
| --- | --- | --- |
| Recurring definition and prompt revision | Swift `AppStateStore` | Pod0 Rust storage |
| Due occurrence and miss-once cadence | Swift `DesiredStatePlanner` | Rust application reducer with injected time |
| Attempt, retry, blocked, cancellation state | Swift `JobStore` | Rust workflow store |
| Provider/tool operation handle | Swift | Native capability adapter, transient only |
| Completion and selected output | Swift chat/artifact stores | Atomic Rust occurrence/artifact commit |
| Conversation presentation and token stream | SwiftUI/session state | Native presentation over bounded Rust projection |
| Credential material | Keychain/provider stores | Native platform security facility |

No completion fact is inferred from a non-empty native message, an accepted
provider request, a closed stream, a file existing, or an NMP receipt. Rust
accepts completion only from a correlated observation for the current attempt
and commits it with the output reference and recurrence update.

## Stable identifiers and time

- `ScheduledTaskId` is the stable recurring-definition identity.
- `ScheduledOccurrenceId` is derived from task id and the exact scheduled
  instant, not launch time. Its representation is versioned.
- `ScheduledAttemptId` fences one provider/tool execution.
- `ScheduledHostRequestId` identifies one native capability request.
- `GeneratedArtifactId` and a content digest identify immutable output.
- Every durable timestamp uses integer milliseconds supplied to the Rust
  application boundary or its injected clock. Native wall-clock reads never
  occur inside replayable policy.

Miss-once cadence remains product policy: after one exact due occurrence
commits, the next run is based on the observed completion instant plus the
definition interval. Missing several periods produces one catch-up occurrence,
not a burst. Updating a definition creates a new prompt revision without
changing the identity or rewriting immutable historical occurrences.

## Typed application contract

Commands are fire-and-forget and produce command receipts/projection state:

- `EnsureScheduledTask`
- `UpdateScheduledTask`
- `RemoveScheduledTask`
- `ReconcileScheduledRuns`
- `CancelScheduledRun`

The bounded projection contains current task definitions and only the selected
occurrence page. Each occurrence exposes its semantic stage, attempt fence,
safe failure state, next eligible time, current host request, and committed
artifact reference. Full conversations, event history, secrets, provider raw
payloads, workflow journals, and databases never cross FFI.

Rust may emit:

```text
ExecuteScheduledAgentTurn {
  request_id,
  occurrence_id,
  attempt_id,
  prompt_revision,
  model_reference,
  bounded_context
}
```

Native reports one typed raw observation carrying all three correlation ids:

- execution accepted;
- bounded completion evidence and immutable output digest;
- raw provider/tool failure;
- cancellation acknowledged.

The adapter does not classify retryability or advance recurrence. Start,
cancel, stop, and restart are idempotent. A stale or duplicate observation is
ignored or recorded diagnostically without changing authoritative state.
Errors and cancellation appear in Rust projection state rather than crossing
FFI as per-operation success or recovery policy.

## Lifecycle and recovery

The lifecycle is:

1. Rust reconciles definitions against injected time and ensures one immutable
   occurrence for each exact due identity.
2. Rust persists and fences an attempt before emitting a host request.
3. Native executes the exact request and reports raw correlated observations.
4. Rust validates the current attempt and output evidence.
5. Rust atomically commits occurrence success, generated-artifact selection,
   and recurrence advancement.
6. On restart, Rust reconstructs definitions, occurrences, active fences, and
   any host work that must be adopted, cancelled, or attempted again.

Provider acceptance alone is not durable completion. If process loss leaves an
external result ambiguous, Rust preserves an explicit ambiguous/recovery state
and never blindly starts a second paid operation. Native never polls Rust or a
provider; capability completion and application wakeups are event-driven.

## Import and authority cutover

Issue #130 performs a one-way inspect, stage, validate, commit sequence under
the legacy persistence writer lock:

1. Inspect scheduled definitions, supported scheduled job rows, completion
   conversations, and scheduled-output artifact records without mutation.
2. Create and verify a versioned, generation/content-qualified rollback backup.
3. Stage stable identities, prompt/model revisions, lifecycle facts, and output
   evidence in Rust.
4. Reconcile duplicates and in-flight work by explicit disposition; malformed,
   conflicting, future-schema, or unverifiable facts fail closed.
5. Revalidate source generation and staged digest while the Swift writer is
   held. Changed input discards staging and restarts inspection.
6. Commit the Rust authority marker before the Rust dispatcher starts.
7. Delete the scheduled-agent branches from Swift planning, job execution,
   retry/recovery, artifact-completion, and recurrence policy.

Before authority commits, rollback discards staging and leaves Swift active.
After authority commits, the old writer cannot be restored. Rollback uses a
separately tested export/restore procedure; the backup remains read-only
evidence and cannot overwrite Rust authority.

## Required falsifiers

- due, not-due, and several-missed-period reconciliation;
- duplicate ensure and duplicate/late host observations;
- cancellation before dispatch, during execution, and after a late callback;
- credential blocking and later raw capability availability;
- process loss before request, during execution, after output staging, and
  before atomic completion;
- stale attempt rejection and exact-once recurrence advancement;
- empty, pending, running, blocked, succeeded, duplicate, malformed,
  future-schema, and interrupted imports;
- generated Swift/Kotlin semantic parity and Android-compatible Rust builds;
- bounded projections, deterministic teardown, and absence of polling;
- deletion of every replaced Swift writer in the cutover issue.

## Consequences

- iOS feature work can keep native provider clients and SwiftUI iteration.
- Android later consumes the same occurrence and recovery behavior instead of
  recreating it in WorkManager or Compose.
- The first implementation is a concrete workflow slice, not a generic job
  framework.
- Chat presentation may remain native, but it cannot remain a durable
  completion authority.
- Nostr publication remains independently recoverable and cannot widen this
  workflow boundary.

## Rejected alternatives

- **Keep recurrence and retry in Swift:** creates unavoidable Android
  duplication and two durable decision owners.
- **Let chat history prove completion:** conflates presentation persistence
  with fenced workflow and artifact state.
- **Dual-write Swift and Rust jobs:** cannot prove ordering after termination.
- **Send arbitrary JSON jobs over FFI:** loses stable identities, validation,
  and forward-compatible typed behavior.
- **Move provider clients into Rust now:** blocks iOS validation and moves
  credential/platform primitives across the wrong boundary.
- **Use NMP as the job store:** leaks Pod0 product nouns into generic protocol
  infrastructure.
