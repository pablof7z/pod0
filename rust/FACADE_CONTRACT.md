# Pod0 native/core contract

This contract is the only allowed boundary between native applications and the
Pod0 product kernel. Rust definitions are authoritative. Swift and Kotlin
bindings are generated from this surface into `Generated/Pod0Core`; native code
must not create a parallel wire model.

## Interaction model

`Pod0ApplicationApi` has seven operations:

1. `dispatch(command)` enqueues a typed semantic command and returns no
   operation result.
2. `snapshot(request)` returns a bounded full projection for initial rendering
   or explicit recovery.
3. `subscribe(request, subscriber)` immediately establishes event-driven
   projection delivery and returns a transient handle.
4. `unsubscribe(handle)` ends delivery deterministically. Dropping a screen
   must call it; native polling is forbidden.
5. `next_host_requests(maximum_count)` drains a bounded batch of native
   capability work.
6. `next_host_cancellations(maximum_count)` drains a bounded batch of native
   cancellation requests.
7. `record_host_observation(observation)` submits raw, correlated capability
   evidence and returns a typed retention receipt. `Persisted` is the only
   acknowledgement that permits native code to discard paid or otherwise
   irreproducible model evidence; `RetainAndRetry` requires the native host to
   keep and redeliver the exact observation.

The application actor introduced with the first vertical slice is the one
writer. Dispatch and observation calls are actor inputs. Reducers do not await
native work.

## Commands and outcomes

Every `CommandEnvelope` carries a stable command ID, cancellation ID, optional
expected state revision, and one typed `ApplicationCommand`. A retry with an
identical command ID and payload is idempotent even if state has advanced. Reuse
of the ID with different content is rejected. A new command with a stale
expected revision becomes a revision-conflict failure.

Accepted, running, blocked, failed, cancelled, and succeeded are semantic
`OperationStage` values in revisioned projections. Terminal failure and
cancellation always clear busy state. `CoreFailure` contains a stable code,
safe display detail, retryability, and an allowed user action. Per-operation
exceptions and dynamic JSON results do not cross the boundary.

## Projections and bounds

Library and playback are screen-shaped projections rather than database or
event-store views. Every envelope has a contract version and state revision.
Requests clamp their item count to `1...MAX_PROJECTION_ITEMS`; operation lists
are capped at `MAX_OPERATION_ITEMS`. Queue and operation construction must
apply the same bound before publication. Full snapshots are the correctness
baseline. Update delivery is coalesced by the actor and may not exceed 60 Hz
for one subscription.

The native player may animate playhead time locally. Only bounded observations
needed for resume, completion, interruption, or queue policy return to Rust.

## Host effects and cancellation

Every `HostRequestEnvelope` carries request, command, cancellation, and issued
revision identities plus an optional absolute deadline. Observations echo the
request ID, cancellation ID, issued revision, a monotonic sequence, and their
observation time. Unknown, duplicate, out-of-order, mismatched, stale,
oversized, expired, or post-cancellation observations cannot commit. Feed bytes
are bounded by the request's declared maximum and carry only HTTP/cache
evidence; feed normalization remains in Rust. The native host reports raw
failure codes; Rust decides retry, fallback, and durable state.

Model-chapter execution follows claim-before-return semantics. Rust durably
authorizes a single POST before an `ExecuteChapterModel` request can leave the
facade. Provider acceptance updates form an ordered, non-terminal stream for
one provider operation ID. A completion is stored as raw immutable evidence
before the facade returns `Persisted { terminal: true }`; qualification,
provenance validation, artifact commit, and source-of-truth selection then run
inside Rust. On restart, an accepted provider operation is recovered by ID and
an authorized request without provider evidence becomes ambiguous rather than
being submitted again. Credentials never cross or persist at this boundary.
Delayed retries and completion-finalization recovery use `ScheduleCoreWake`;
native schedules the requested time and echoes the typed reason, while Rust
retains all retry policy and durable workflow state. Provider generation time
is optional evidence; when absent, Rust assigns injected kernel time.

Playback uses typed load/play/pause/seek/rate/timer requests. A long-lived
`ObservePlayback` request returns lifecycle boundaries immediately and
coalesces position-only updates to `500...5000 ms`; its sequence remains open
until explicit cancellation. AVFoundation route names and errors are mapped to
the bounded generated vocabulary. UI playhead animation never uses this stream.
This expansion began with contract version 2. Version 4 added Rust-owned
playback commands and projections for selection, queue, resume, completion,
rate, bounded segments, preferences, and session sleep timers. Contract version
17 adds selected-artifact chapter context, deterministic next/previous chapter
actions, bounded per-session ad suppression, and seek reasons. A chapter seek
always carries episode, artifact, selection revision, session, policy version,
and request identity; AVFoundation and future Media3 hosts execute the exact
target without inspecting or recomputing the policy.

## Compatibility rules

- IDs use typed 128-bit values represented as two unsigned 64-bit words.
- Times and positions name their units explicitly; no platform date or duration
  type crosses the boundary.
- Additive record fields require defaults before use. Removing or changing a
  field, unit, or meaning requires a contract-version change and compatibility
  fixture.
- Enums include an `Unsupported { wire_code }` case. Unknown persisted or
  remote codes map there and become a safe unsupported operation/projection.
- Generated bindings and the linked Rust binary are one versioned artifact;
  mixing independently generated bindings is unsupported and rejected by CI.
- Collections are bounded. Secrets, database rows, workflow journals, signer
  state, native framework objects, and high-frequency animation state never
  appear in projections.

## Current authority

The facade opens the versioned authoritative Rust `LibraryStore`. Podcast,
subscription, episode listening, active playback, queue, resume, completion,
rate, playback preferences, and sleep mode are durable Rust-owned state. The
Swift shell imports the legacy listening snapshot once, renders bounded
library/playback projections, executes URLSession and AVFoundation requests,
and cannot commit migrated facts after cutover. Selected transcripts, canonical
chapters, and both publisher and model chapter workflow decisions are also
durable Rust-owned state. Download intent, attempts, recovery, and artifact
selection are also Rust-owned; Swift executes background URLSession transfers.
Swift still owns transcript-generation/index workflow scheduling, remaining
agent workflow state, and presentation state until their complete vertical
slices land. The audited NMP pin is available only through the isolated
`pod0-nmp` adapter until a Pod0-specific Nostr vertical slice composes it into
this facade.

Canonical chapter artifacts and selections are Rust-owned after the chapter
cutover. Contract version 24 adds durable source-version provenance to the
model-chapter command, projection, host-request, observation, and receipt
surface. Contract version 25 adds the typed legacy-workflow authority cutover:
iOS stages current, reconstructable semantic candidates in Rust while a native
rollback manifest preserves every exact legacy row. It durably verifies that
manifest, atomically compare-deletes only the matching Swift chapter-model jobs,
verifies their absence, and only then commits the Rust authority marker. Exact
current success receipts are adopted; reconstructable interrupted or otherwise
uncertain submissions remain explicitly ambiguous and dormant until a
user-authorized retry. Stale and unplannable rows remain only as rollback evidence
instead of becoming a second Rust workflow format. Startup resumes a staged
cutover before the native dispatcher can execute any request.
If the legacy source changes before commit, native may discard only the exact
staged source generation. Rust atomically verifies and removes only records
attributed to that stage plus its marker; a missing, mismatched, or authoritative
stage fails closed. Both staged and not-started states reject model workflow
commands and host dispatch, so discard never opens a temporary authority window.
The native iOS model adapter remains native by design, while all durable model
workflow decisions now have one Rust source of truth.

Contract version 30 defines the next transcript-workflow boundary before its
durable cutover. Stable workflow, attempt, and submission-fence identities;
generation and evidence-index desired-state planning; retry classification;
bounded workflow projections; and publisher/remote/Apple capability payloads
are Pod0-owned Rust types. Ambiguous paid submissions are never classified as
safe to resubmit: an accepted provider identity can only be recovered, while an
authorized request with no provider evidence requires explicit resolution.
Native code will retain credentials and provider/Apple execution, but it cannot
own fallback, retry timing, identity, or artifact-selection policy.

Contract version 31 integrates that capability boundary with the shared host
ledger. Every transcript capability carries the exact episode, podcast, source
revision, attempt, and submission fence selected by Rust. Native returns one
bounded typed observation; completed artifacts must match the requested
context, provider recovery cannot change the external operation identity, and
the ledger rejects stale, cancelled, malformed, oversized, or mismatched
observations before durable workflow state can advance.

Contract version 32 adds a Rust-owned transcript speaker identity helper.
Native providers may supply unstable UUIDs, but canonical transcript artifacts
derive speaker identity from the episode, source revision, and provider label
so replay produces the same durable artifact.
