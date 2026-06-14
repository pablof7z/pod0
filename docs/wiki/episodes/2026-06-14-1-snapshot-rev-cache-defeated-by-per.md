---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-serialization
  - perf-hot-path
  - rev-cache
supersedes:
  - 2026-06-14-1-snapshot-cache-defeated-by-per-tick
related_claims: []
source_lines:
  - 30-57
  - 122-161
captured_at: 2026-06-14T04:44:55Z
---

# Episode: Snapshot rev-cache defeated by per-tick rev bump

## Prior State

The rev-gated snapshot cache in `build_snapshot_payload` was believed to make snapshot serialization cheap — only re-serializing when state changes, otherwise returning a cached JSON string.

## Trigger

Profiling process 21680 showed 57% of CPU samples in `build_snapshot_payload` → `serde_json::to_string`, with 14.6GB physical footprint. Inspection confirmed the cache logic is correct but `rev` bumps on essentially every actor tick (from comments_handler, feed_fetch, knowledge, agent_note, etc.), invalidating the cache each tick.

## Decision

Root cause diagnosed: the rev counter is too granular — it increments on every handler action, not just on state that affects the serialized payload. No fix was implemented in this session; the three suggested approaches are: (1) delta snapshots — serialize only changed podcasts/episodes, (2) cache the serialized form per-podcast, (3) push individual PodcastSummary updates instead of the full PodcastUpdate envelope per tick.

## Consequences

- Every command dispatch triggers full-library JSON re-serialization on the actor thread (a D8 violation — must be cheap, non-blocking)
- 14.6GB memory footprint likely from accumulated allocations from repeated full-library serializations
- The leaf bottleneck (format_escaped_str_contents) is serde doing its job on an oversized payload — the real fix is reducing what gets serialized per tick

## Open Tail

- No fix implemented in this session; the three approaches remain uncommitted
- Which approach (or combination) to adopt is a product/architecture decision pending user input

## Evidence

- transcript lines 30-57
- transcript lines 122-161

