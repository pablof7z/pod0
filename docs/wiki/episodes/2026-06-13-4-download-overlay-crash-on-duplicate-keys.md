---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - download-overlay
  - uniquekeyswithvalues
  - kernel-projection
supersedes: []
related_claims: []
source_lines:
  - 10067-10072
  - 10098-10102
captured_at: 2026-06-13T21:21:24Z
---

# Episode: Download-overlay crash on duplicate keys — last-writer-wins replaces fatalError

## Prior State

AppStateStore+KernelProjection.swift used uniqueKeysWithValues to build a Dictionary from kernel projection data. If the kernel briefly carries duplicate download IDs (e.g., during a state transition), this causes a fatalError that crashes the app mid-playback.

## Trigger

The crash manifested during playback. Investigation showed the kernel can transiently emit duplicate download IDs in its projection data, and Swift's uniqueKeysWithValues fatally errors on duplicate keys rather than gracefully handling them.

## Decision

Replace uniqueKeysWithValues with uniquingKeysWith:{_, last in last} (last-writer-wins) so duplicate keys are resolved instead of crashing. Merged as #442 by the autonomous fleet.

## Consequences

- Duplicate download IDs from the kernel no longer crash the app; the last value wins
- The underlying kernel-side duplicate-key emission still exists transiently but is now tolerated rather than fatal
- This is a defensive fix — the root cause (kernel emitting duplicate IDs) could still be addressed upstream

## Open Tail

- Whether the kernel should deduplicate download IDs before emitting projections, or whether transient duplicates are an expected state

## Evidence

- transcript lines 10067-10072
- transcript lines 10098-10102

