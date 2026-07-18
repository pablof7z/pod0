# Pod0 native/core contract

This contract is the only allowed boundary between native applications and the
Pod0 product kernel. Rust definitions are authoritative. Swift and Kotlin
bindings will be generated from this surface in issue #76; native code must not
create a parallel wire model.

## Interaction model

`Pod0ApplicationApi` has six operations:

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
6. `record_host_observation(observation)` submits raw, correlated capability
   evidence and returns no product decision.

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
revision identities. Observations must echo the request ID, cancellation ID,
and issued revision. Unknown, duplicate, mismatched, stale, oversized, or
post-cancellation observations cannot commit. Feed bytes are bounded by the
request's declared maximum. The native host reports raw failure codes; Rust
decides retry, fallback, and durable state.

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

## Current bootstrap limitation

The contract types, idempotency ledger, host-observation correlation, bounds,
and subscription lifecycle are implemented and tested. No product domain has
cut over: Swift remains the source of truth until the complete first listening
slice imports data, enables the Rust actor, verifies parity, and deletes the
replaced Swift ownership. The NMP adapter remains isolated by the security hold
in issue #85.
