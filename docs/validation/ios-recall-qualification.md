# iOS recall qualification

Status: required evidence for the Rust-authoritative recall cutover and the M5
iOS validation gate.

## Ownership invariants

- Rust owns selected transcript generations, stable spans, provenance, ranking,
  fallback, cancellation, and terminal recall state.
- Swift may render bounded projections, hydrate display-only metadata, seek the
  native player to a projected millisecond anchor, and execute typed embedding,
  vector/lexical, and reranking capability requests.
- The native vector/FTS database is disposable. It is rebuilt only from the
  Rust-selected evidence projection and is never a rollback authority.
- Recall is event-driven. No timer, sleep/check loop, shadow result, or alternate
  Swift ranking/citation path participates in production.

## Performance budgets

The deterministic local budget measures the native vector plus lexical
candidate capability after the query embedding exists. On the current iOS
Simulator CI baseline, a warmed 5,000-span corpus must complete at p95 below
100 ms across 20 queries. Five thousand one-minute spans approximate eighty
hour-long prepared episodes. `CoreRecallPerformanceTests` constructs that corpus,
runs both retrieval lanes, and enforces the limit.

Provider time is measured separately because it depends on the user's selected
service and network. Product qualification requires at least 95% of completed
grounded/no-evidence recalls in the under-five-second signal buckets, as defined
in `product-proof-metrics.md`. Provider timeouts or failures must become typed
`providerUnavailable` state; they cannot silently fall back to a Swift result.

## Required automated evidence

- Exact IDs, digest, provenance, order, excerpts, and timestamps survive the
  Rust → Swift and Rust → Kotlin golden fixtures.
- Prepared transcript rebuild is idempotent and preserves the selected source
  file through facade restart.
- Cancellation returns the kernel's `cancelled` projection and cancels the
  correlated native request.
- Missing transcript/index, indexing, no evidence, provider/index unavailable,
  corrupt artifact, cancellation, and interrupted restart states are explicit.
- Playback begins at the exact projected millisecond anchor.
- Generated bindings, Android Rust targets, architecture checks, the complete
  Rust workspace, and the complete iOS test suite pass.

## Runtime qualification

Launch a simulator build, open the Recall surface, and verify its empty or
prepared-library state renders without a crash, blank screen, polling, or raw
provider error. A prepared-corpus live provider run is additional environmental
evidence; compilation alone does not prove that provider path.
