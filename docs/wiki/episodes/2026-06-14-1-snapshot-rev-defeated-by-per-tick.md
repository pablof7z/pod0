---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - snapshot-perf
  - build-snapshot-payload
  - rev-cache-invalidation
supersedes:
  - 2026-06-14-1-snapshot-rev-cache-defeated-by-per
related_claims: []
source_lines:
  - 30-57
  - 122-161
captured_at: 2026-06-14T05:49:17Z
---

# Episode: Snapshot rev defeated by per-tick bumps — full-library re-serialization on every dispatch

## Prior State

build_snapshot_payload had a rev-gated cache intended to skip re-serialization when nothing changed

## Trigger

57% of CPU samples in serde_json::to_string inside build_snapshot_payload; 14.6 GB memory footprint

## Decision

Root cause identified: rev is bumped on essentially every actor tick by multiple handlers (comments, knowledge, feed_fetch, agent_note, etc.), defeating the cache entirely — the full library re-serializes on every command dispatch. The leaf bottleneck (format_escaped_str) is serde doing its job on a huge payload; the real fix must reduce what gets serialized per tick (delta snapshots, per-podcast cache, or individual PodcastSummary pushes instead of the full PodcastUpdate envelope)

## Consequences

- The performance problem is not a serde inefficiency but an architectural one: rev invalidation is too coarse-grained
- Any handler that bumps rev (comments, knowledge, feed_fetch, agent notes) triggers a full-library re-serialization
- 14.6 GB memory footprint likely stems from accumulating allocations from repeated full-library serializations
- Three viable fix directions: delta snapshots, per-podcast serialized-form cache, or structural push of individual PodcastSummary updates

## Open Tail

- No fix implemented yet — the three approaches (delta, per-podcast cache, structural push) need prioritization
- perf_ffi_snapshot_transport.md already tracks this issue

## Evidence

- transcript lines 30-57
- transcript lines 122-161

