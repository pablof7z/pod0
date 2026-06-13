---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - snapshot-serialization
  - podcast-update
  - rev-cache-defeat
supersedes:
  - 2026-06-13-1-snapshot-cache-defeated-by-global-rev
related_claims: []
source_lines:
  - 30-57
  - 122-161
  - 9992-10008
captured_at: 2026-06-13T21:48:25Z
---

# Episode: Snapshot full-library rebuild killed — slice-local payload builders replace per-tick serialization

## Prior State

Every kernel tick re-serialized the entire podcast library (all podcasts × all episodes) via serde_json::to_string in build_snapshot_payload, with a rev-gated cache intended as a fast path. The cache was sound in theory but defeated in practice because rev bumped on essentially every command dispatch from multiple handlers (comments, feed_fetch, knowledge, agent_notes, etc.), causing full re-serialization on every tick — 57% of CPU samples and a 14.6 GB physical footprint.

## Trigger

CPU profiling of process 21680 showed 1,633/2,856 samples (~57%) in build_snapshot_payload → serde_json::to_string; investigation of the rev-cache revealed it was structurally defeated by frequent rev bumps, making every tick a full-library serialization.

## Decision

Replaced the whole-library-per-tick pattern with slice-local payload builders — each projection domain builds only its own delta, eliminating the need to re-serialize the entire library on every command dispatch.

## Consequences

- Eliminated the 57% CPU hot path in the actor thread
- Reduced the 14.6 GB physical footprint caused by accumulated serialization allocations
- The rev-gated cache remains for the pull path but is no longer the primary performance mechanism for the push path
- Each projection domain now owns its own serialization scope rather than depending on a monolithic PodcastUpdate envelope

## Open Tail

- Whether the rev-gated cache should also be removed from the pull path or kept as a fallback
- Whether delta snapshots or cached-serialized-form should be the long-term approach for individual slices

## Evidence

- transcript lines 30-57
- transcript lines 122-161
- transcript lines 9992-10008

