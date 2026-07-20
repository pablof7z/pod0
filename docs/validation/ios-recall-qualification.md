# iOS recall qualification

Status: production evidence for the Rust-authoritative recall cutover and the
M5 iOS validation gate. The recorded run is in
`docs/architecture/evidence/recall-index-cutover-2026-07-20.json`.

## Ownership invariants

- Rust owns selected transcript generations, the versioned disposable index,
  embedding-cache identity, vector/lexical retrieval, ranking, fallback,
  cancellation, citations, and terminal recall state.
- Swift may render bounded projections, seek AVFoundation to a projected
  millisecond anchor, execute typed embedding/reranking provider requests, and
  remove the three exact legacy index artifacts when Rust requests it. It does
  not decide cutover, query an index, or return raw candidate lanes.
- Canonical evidence stays in the Rust core store. The sibling
  `.recall-index.sqlite` file is reconstructible and is never rollback authority.
- Recall remains event-driven. No polling, shadow result, dual write, or
  alternate Swift ranking/citation path participates in production.

## Performance budgets

Run the production backend in release mode:

```sh
cd rust
cargo run -p pod0-recall-index --bin pod0-recall-index-benchmark \
  --release --locked -- --spans 5000 --dimensions 1024 --samples 20
```

Required limits are rebuild below 5 seconds, cold query below 150 ms, warm p95
below 100 ms, index below 75 MB, and at most 40 candidates. Provider time is
measured separately because it depends on the selected service and network.
Provider failures become typed state and never fall back to a Swift result.

## Required automated evidence

- The golden fixture preserves candidate identities and raw lane ranks; Rust
  preserves final playable evidence, provenance, timestamps, and citations.
- Cached embeddings rebuild a deleted execution index after restart without
  provider work; missing embeddings produce bounded batches of at most 16.
- Cancellation enters SQLite, interrupts within 50 ms, returns typed cancelled
  state, and commits no partial generation.
- Corrupt disposable artifacts self-heal without touching canonical evidence;
  newer schemas fail closed.
- A populated legacy `vectors.sqlite` remains until every selected active-library
  Rust generation and exact span coverage verifies. Rust then requests bounded
  native deletion of only its database/WAL/SHM artifacts and commits the marker
  only after the correlated typed observation; unresolved locations, directories,
  and symlinks fail closed without substituting a temporary path.
- Generated Swift/Kotlin bindings are drift-free and Apple-free; Apple device
  and simulator plus Android API 23 ARM64/x86_64 targets build.
- Full Rust, iOS Debug, and iOS Release suites pass. Release tests retain release
  optimization and enable testability only for the XCTest invocation.

## Runtime qualification

Build, install, and launch the app with XcodeBuildMCP on the iOS simulator.
Verify the normal app surface renders without a crash, blank screen, polling,
or raw provider error. Run the production recall binary on Android API 34 ARM64
as the shared-runtime smoke. A live credentialed provider run is additional
environmental evidence and is not replaced by compilation.
