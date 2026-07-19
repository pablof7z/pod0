# ADR-0003: Typed UniFFI application facade

- Status: Accepted
- Date: 2026-07-18
- Decision owners: Pod0 application architecture
- Related issues: #57, #63, #74, #76

## Context

There is no current native/shared bridge. Multiple bridges, manual model
mirrors, or arbitrary JSON RPC would make schema evolution and single ownership
unreliable. The facade must preserve native responsiveness and be implementable
by both Swift and Kotlin.

## Implementation status

The generated Swift/Kotlin APIs and durable Rust runtime now own the migrated
listening, notes, clips, and recall/evidence slices. CI regenerates both
languages to detect drift. Contract version 11 adds a canonical full-transcript
artifact plus separately bounded summary/speaker/segment/word projections. A
pure, bounded contract projection lets both bindings prove IDs, limits,
unknown-value handling, and conversion fixtures before storage cutover; it
neither dispatches an application operation nor writes durable state. Invalid
fixture input is represented as rejected projection state, never an exception.

## Decision

Pod0 exposes exactly one app-owned UniFFI facade. It uses typed, versioned:

- application commands and internal events;
- bounded screen/view projections with stable IDs and revisions;
- domain events for committed facts;
- host requests and raw host observations;
- semantic action stages, failures, diagnostics, and cancellation state.

`dispatch(command)` is fire-and-forget and enqueues work on one Rust application
actor. It does not synchronously return operation success. Reducers do not
await. Async work and native effects report back as explicit events or host
observations, which the actor processes as the single writer.

## Projection and reactivity contract

- Only open app chrome/views receive data.
- Projections are bounded, screen-shaped, revisioned, and replaceable.
- Full bounded snapshots are the correctness baseline; lossless deltas require
  measurement and a documented benefit.
- Native subscribes/unsubscribes explicitly; no polling or sleep-check loop.
- Update delivery is coalesced and never exceeds 60 Hz per view.
- Event history, databases, watermarks, signer state, and workflow journals do
  not cross FFI.
- High-frequency playhead animation remains native. Rust receives bounded raw
  observations needed for durable resume/completion decisions.

## Errors and cancellation

Per-operation exceptions or `Result<T, E>` do not cross FFI. A command has a
stable command/cancellation ID. Its accepted/running/blocked/failed/cancelled/
succeeded stage appears in the relevant revisioned projection. Busy state must
always clear on terminal failure or cancellation.

Native host observations may carry typed raw failure codes and safe metadata.
Rust decides retry, fallback, user action availability, and durable next state.
Late or duplicate observations are rejected by request ID, revision, lease, or
idempotency evidence.

Pure migration inspection helpers that predate this facade are limited to
offline cutover tooling. New contract-qualification surfaces return bounded
state projections, including typed rejection state, and never throw across
FFI. Once a domain is authoritative, all user operations follow the
fire-and-forget command/projection rule.

## Compatibility

- Stable opaque IDs and explicit time/size units cross FFI.
- Unknown future enum variants degrade to safe unsupported state.
- Generated Swift and Kotlin bindings derive from the same facade revision.
- CI regenerates bindings and rejects drift.
- Shared records contain no UIKit, AVFoundation, URLSession, Swift `Date`,
  Android framework, or platform path types.

## Migration and rollback

The facade is additive until a complete domain cutover. A feature flag may
select the existing Swift owner before cutover. Shadow reads may compare
projections, but duplicate host side effects and durable dual writes are
forbidden. Experimental unused facade types are deleted rather than preserved.

## Consequences

- Native UI never blocks on a synchronous Rust operation.
- User-visible operation outcomes are observable and restart-safe.
- Projection shape and cadence are part of performance correctness.
- Native adapters stay small but remain ergonomic for SwiftUI and Compose.

## Rejected alternatives

- **Arbitrary JSON/string method RPC:** loses compile-time schema and error
  guarantees.
- **Synchronous command results:** misrepresent durable asynchronous work and
  create blocking/cancellation races.
- **One bridge per feature:** creates incompatible ownership seams.
- **Send the full app/event store:** violates bounded memory and privacy.
- **Native polling:** wastes power and makes state convergence timing-dependent.
