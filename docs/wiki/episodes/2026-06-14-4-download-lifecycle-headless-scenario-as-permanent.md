---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - download-projection
  - headless-scenarios
  - ci-regression-net
supersedes: []
related_claims: []
source_lines:
  - 12073-12085
captured_at: 2026-06-14T07:49:05Z
---

# Episode: Download lifecycle headless scenario as permanent CI regression net

## Prior State

The download projection crashed twice in production (#442 duplicate keys, #463 restored-downloads-visibility) yet had zero headless scenario coverage. The full action seam existed (`download`/`cancel_download`/`pause_download`/`resume_download`/`cancel_all_downloads`) but was completely untested in the headless suite (13 scenarios, none touching downloads).

## Trigger

Cycle-18 planner confirmed no headless scenario covers the download lifecycle, and that this is the most crash-prone projection in the tree — the exact regression class that CI should permanently guard.

## Decision

Create a new `scenarios/download.rs` registered in the headless suite, covering download materialize/pause/resume/cancel + asserting that restored/completed rows stay visible (the exact #463 regression). CI gate #449 runs the headless suite, making this a permanent regression net.

## Consequences

- The most crash-prone projection is permanently CI-gated against the exact regression classes from #442 and #463
- Follows the Skip-aware pattern for network-dependent steps (degrades gracefully in CI)
- File boundary is minimal: `download.rs` + one registry line in `mod.rs` — fully parallel-safe

## Open Tail

*(none)*

## Evidence

- transcript lines 12073-12085

