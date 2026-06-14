---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - snapshot-performance
  - podcast-update-serialization
  - rev-gating
supersedes: []
related_claims: []
source_lines:
  - 30-57
  - 122-161
captured_at: 2026-06-14T05:59:05Z
---

# Episode: Snapshot serialization rev-defeat: cache exists but is defeated by per-tick rev bumps

## Prior State

The rev-gated snapshot cache in build_snapshot_payload was believed to make serialization cheap on unchanged data; perf issues were assumed to need a cache added

## Trigger

Profiling showed 57% of samples in build_snapshot_payload → serde_json::to_string with 14.6 GB physical footprint; investigation revealed the cache logic is correct but rev bumps on every actor tick (comments_handler, knowledge, feed_fetch, agent_note_handler, etc.), invalidating the cache each time

## Decision

Root cause identified: the performance problem is not missing caching but excessive rev bumps that defeat the existing cache. The fix must target rev-bump frequency or structural serialization change (delta snapshots, per-podcast serialization), not add caching that already exists

## Consequences

- Future perf work must reduce what triggers rev bumps or what gets re-serialized per tick, not add another cache layer
- The 14.6GB footprint is likely from accumulated full-library serialization allocations on every tick
- String escaping (format_escaped_str) is the leaf bottleneck but is serde doing its job on a huge payload — the real fix is reducing payload size per tick

## Open Tail

- No fix adopted this session — three options remain: delta snapshots, cached-per-podcast serialization, or structural per-item push updates

## Evidence

- transcript lines 30-57
- transcript lines 122-161

