---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-serialization
  - rev-gated-cache
  - build-snapshot-payload
supersedes:
  - 2026-06-14-1-snapshot-cache-defeated-by-global-rev
related_claims: []
source_lines:
  - 30-57
  - 122-161
captured_at: 2026-06-14T04:04:34Z
---

# Episode: Snapshot cache defeated by per-tick rev bumps

## Prior State

The rev-gated snapshot-string cache in `build_snapshot_payload` was assumed to make snapshot serialization cheap — an unchanged rev would return a cached clone instead of re-serializing the entire library.

## Trigger

Profiling process 21680 showed 1,633/2,856 samples (~57%) in `build_snapshot_payload` → `serde_json::to_string`, plus 14.6 GB physical footprint. Investigation of the cache code revealed it is correct — but `rev.fetch_add(1, Ordering::Relaxed)` is called from comments_handler, feed_fetch, knowledge, agent_note_handler, and other handlers on essentially every actor tick, defeating the cache entirely.

## Decision

Root cause identified but not yet resolved in this session. Three durable fixes proposed: (1) delta snapshots — serialize only changed podcasts/episodes; (2) cache the serialized form per-podcast — only re-serialize when that podcast changes; (3) structural change — push individual `PodcastSummary` updates rather than the full `PodcastUpdate` envelope on every tick.

## Consequences

- Explains both the CPU peg (57% in serde) and the 14.6 GB memory footprint (accumulating allocations from repeated full-library serializations)
- The string-escaping leaf hotspot (`format_escaped_str_contents`) is serde doing its job on a huge payload — optimizing serde itself is the wrong fix
- Any fix must reduce what gets serialized per tick, not just cache the result

## Open Tail

- Which of the three approaches (delta, per-podcast cache, or structural push) to adopt
- Whether the rev counter should be made more granular (per-podcast instead of global) to preserve the existing cache path

## Evidence

- transcript lines 30-57
- transcript lines 122-161

