# Projection batch performance

Hot native reads use `ProjectionBatchRequest` to obtain up to 16 ordered,
individually paginated projections while the facade holds one state lock. Every
envelope in the result carries the same state revision. Each nested request is
still limited to 200 items, and oversized batches return the first 16 requests
with `has_more` set.

## Reproducible budgets

Run the baseline screen-read proof in the optimized profile:

```sh
cd rust
cargo test --release -p pod0-facade \
  runtime_projection_batch_tests::baseline_projection_batch_stays_within_documented_budget \
  -- --exact --nocapture --test-threads=1
```

The batch contains the four common library, playback, note, and memory reads.
After three warm-ups, 20 samples must have a p95 below 25 ms. The debug-profile
guard is 250 ms.

On 2026-09-07, the focused release run on the development machine measured a
250 ns median and a 375 ns p95.

The supported large-history proof remains the 10,000-fact newest-first activity
query described in `activity-journal-performance.md`. It decodes at most 200
items and must stay below 250 ms p95 in release:

```sh
cd rust
cargo test --release -p pod0-storage \
  activity_store_latest_tests::ten_thousand_fact_latest_page_stays_bounded_and_within_budget \
  -- --exact --nocapture --test-threads=1
```

Both commands are host-independent, retain no native cache, and fail if either
collection bounds or the documented wall-clock budget regress.

On the same run, the 10,000-fact query measured a 33.817 ms median and a
35.202 ms p95.
