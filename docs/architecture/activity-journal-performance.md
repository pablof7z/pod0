# Activity journal projection performance

The episode diagnostics projection has an explicit bounded newest-first contract:

- one page returns at most 200 activity facts;
- a cursor pins the episode-linked maximum sequence observed on the first page;
- later tail appends do not expand that snapshot;
- invalid or future cursors fail closed.

## Reproducible budget

Run the storage proof in the optimized profile:

```sh
cd rust
cargo test --release -p pod0-storage \
  activity_store_latest_tests::ten_thousand_fact_latest_page_stays_bounded_and_within_budget \
  -- --exact --nocapture --test-threads=1
```

The fixture inserts 10,000 episode-linked facts, performs three warm-up reads, then records 20
samples. Each timed operation opens the validated store, resolves the episode-scoped snapshot, and
decodes the newest 200 facts. Release-profile p95 must remain below 250 ms. The debug-profile test
also carries a five-second CI runaway guard, but that guard is not the performance budget.

On 2026-08-13, the focused release run on the development machine measured a 38.768 ms median and a
42.173 ms p95. A concurrent single-sample debug run measured 1.164 s, illustrating why the debug
guard is deliberately separate. A deterministic `EXPLAIN QUERY PLAN` test additionally requires
SQLite to use the episode/sequence and activity-id indexes. Cardinality and plan assertions prove
bounded behavior without relying on wall-clock timing alone.
