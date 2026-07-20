# ADR-0006: Shared Rust recall-index execution

- Status: Accepted
- Date: 2026-07-20
- Decision owners: Pod0 knowledge and application architecture
- Related issues: #59, #61, #105, #106

## Context

Rust already owns canonical transcript evidence, selected generations, stable
span identities, query workflow, RRF policy, reranking orchestration, final
citations, and recall projections. The current Swift `VectorIndex` owns a
disposable sqlite-vec/FTS execution index and returns exact IDs plus raw vector
and lexical lane ranks. It owns no canonical evidence.

That temporary boundary preserved iOS velocity, but leaving it in Swift would
make Android reproduce schema, scoped retrieval, rebuild, recovery, limits,
and cancellation behavior. Issue #105 therefore compared:

1. a Pod0-owned Rust sqlite-vec/FTS backend with native provider adapters; and
2. a retained typed native execution capability on each platform.

Both candidates consumed the same stable Pod0 identities and bounded evidence
shape. A shared JSON fixture proves Swift sqlite-vec v0.1.6 and Rust sqlite-vec
v0.1.9 return the same exact raw candidate contract for non-tied vector scores,
then proves the existing Rust RRF produces the expected final span ordering.
The facade's recall vertical-slice tests resolve those stable spans back to the
canonical playable evidence and citations. Rust remains the final ranking and
citation authority in either design.

## Decision

The production target is a **Rust-owned recall execution index**. Issue #106
will promote the prototype through a complete cutover; this ADR does not make
the prototype a production source of truth.

Rust will own:

- the disposable index schema and its version;
- rebuild, verification, deletion, corruption recovery, and desired state;
- scoped vector/lexical retrieval, candidate bounds, and raw lane ranks;
- cancellation and safe execution diagnostics;
- all existing final ranking, reranking policy, citations, and projections.

Native code will continue to own:

- provider credentials and platform network execution for embeddings/reranking;
- native UI and transient presentation;
- thin typed adapters that execute bounded Rust host requests.

The index remains reconstructible. Canonical evidence and selected transcript
state stay in the existing Rust store. No native or Rust execution index may
become a second authority for transcript text, evidence identity, or citations.

## Evidence

Measurements are in
[`recall-index-boundary-2026-07-20.json`](../evidence/recall-index-boundary-2026-07-20.json).
They used an Apple M2 Mac mini with 24 GB RAM, iOS 26.5 Simulator, Xcode 26.6,
Rust 1.93.0, 1,024-dimensional vectors, 100 spans per episode, and 20 warm
samples. Both Apple candidates executed inside the same simulator. Values below
are rounded release measurements.

| Spans | Backend | Rebuild | Warm p95 | Cold | Index |
|---:|---|---:|---:|---:|---:|
| 500 | Swift v0.1.6 | 102 ms | 7.1 ms | 8.9 ms | 21.4 MB |
| 500 | Rust v0.1.9 | 18.8 ms | 1.2 ms | 1.5 ms | 3.0 MB |
| 5,000 | Swift v0.1.6 | 2.29 s | 49.8 ms | 49.3 ms | 213 MB |
| 5,000 | Rust v0.1.9 | 171 ms | 8.5 ms | 9.0 ms | 29.2 MB |
| 20,000 | Swift v0.1.6 | 30.9 s | 204 ms | 543 ms | 854 MB |
| 20,000 | Rust v0.1.9 | 711 ms | 35.0 ms | 34.6 ms | 117 MB |

The main size difference is explicit `chunk_size=128` in the Rust schema;
sqlite-vec otherwise allocates its default 1,024-vector chunk for each episode
partition. Native code could adopt that tuning too, so performance alone is not
the ownership argument. It proves the shared candidate has sufficient headroom
and makes durable schema tuning explicit in the intended owner.

The native query task completed after cancellation in every measured dataset;
the 20,000-span response took 167 ms after cancellation. The Rust prototype
returned typed cancellation during its preflight check in under one microsecond
and links its token to SQLite interruption for in-flight work. Issue #106 must
add a deterministic in-flight cancellation test before production cutover.

The Rust binary executed successfully inside iOS 26.5 Simulator and an Android
14/API 34 ARM64 emulator. It initialized sqlite-vec v0.1.9, rebuilt 500 vectors,
performed hybrid retrieval, returned at most 40 exact candidates, and surfaced
typed cancellation. Release binaries also compiled for:

- Apple iOS device ARM64;
- Apple iOS Simulator ARM64 and x86_64;
- Android API 23 ARM64 and x86_64.

The prototype dependency graph contains no network client. Synthetic private
fixture text remains local, error/debug tests reject content leakage, and the
benchmark emits only counts and timings.

Binary-size values are recorded but are not treated as directly comparable.
The Rust value is a standalone evidence executable, while the native value is
the complete iOS app process and Swift package objects. The current Release
simulator build contains about 1.63 MB of ARM64 `CSQLiteVec` object code plus an
85 KB Swift wrapper. The Rust evidence executable is 2.47 MB on iOS Simulator
and includes its CLI plus application/domain dependencies. Issue #106 must
measure the signed app before and after cutover; the expected benefit is removal
of the duplicate Swift SQLite/sqlite-vec package because `Pod0Core` already
links bundled SQLite.

Peak-resident measurements are also process envelopes, not an incremental
ownership comparison: roughly 27–28 MB for the standalone Rust simulator
process and 318–323 MB for the app-hosted XCTest process.

## Production budgets

The cutover must ratchet these release budgets on the iOS simulator fixture:

- 5,000 spans at 1,024 dimensions rebuild in under 5 seconds;
- warm hybrid p95 is under 100 ms and cold query is under 150 ms;
- the 5,000-span execution index is under 75 MB;
- candidate output never exceeds the declared maximum of 40;
- cancellation is typed and completes within 50 ms in a deterministic
  in-flight test;
- no private transcript text appears in diagnostics, logs, or analytics.

These are guardrails, not product-performance targets for every device. Issue
#106 must add physical-device evidence and may tighten budgets, but it may not
weaken them without a superseding ADR.

## Typed boundary

Rust requests bounded embeddings keyed by stable span or query IDs and a
cancellation ID. Native returns ordered, quantized vectors or a typed provider,
timeout, invalid-response, or cancellation result. Rust never asks native code
to select a generation, rank candidates, retry, fall back, or commit state.

Only the Rust-ranked `RecallResultProjection` crosses back for presentation.
No arbitrary JSON RPC, Apple type, high-frequency UI state, or unbounded text
collection enters the shared API.

## Migration and rollback

Issue #106 will use one writer and a one-way ownership marker:

1. stop Swift index writes;
2. create the Rust index only from canonical Rust evidence and valid cached
   embeddings, requesting missing embeddings through the typed host;
3. verify selected generation, exact span coverage, schema version, and query;
4. atomically mark Rust ownership;
5. delete the Swift execution index and its SQLiteVec dependency.

Before the marker, rollback may run the old app. After the marker, an old app
must treat its disposable index as missing and rebuild; it must never roll back
canonical evidence. There is no dual-write steady state.

## Consequences

- Android later consumes the same rebuild/retrieval behavior instead of
  implementing it in Kotlin.
- Native provider and UI quality remain unconstrained by the shared index.
- Rust gains responsibility for an additional disposable schema and migration.
- sqlite-vec remains pre-1.0 and pinned; dependency review and portability
  checks are required on every upgrade.
- Swift `VectorIndex` and the SQLiteVec package become deletion targets in #106.

## Rejected alternatives

- **Retain a native typed index:** technically viable, but duplicates durable
  schema/rebuild/recovery/cancellation behavior and already lacks cancellation.
- **Move embedding network providers into Rust:** harms platform credential and
  provider iteration without improving durable ownership.
- **Remote recall service:** violates the offline/privacy posture and introduces
  an unnecessary server authority.
- **Generic vector abstraction now:** one proven Pod0 use case is insufficient
  to justify a backend framework.
- **Dual-write during migration:** makes disposable state authoritative and
  creates ambiguous recovery.
