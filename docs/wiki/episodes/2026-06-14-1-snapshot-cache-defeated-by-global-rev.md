---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-cache
  - rev-bump-frequency
  - build-snapshot-payload
supersedes:
  - 2026-06-14-1-snapshot-rev-gated-cache-defeated-by
related_claims: []
source_lines:
  - 30-57
  - 122-161
  - 163-171
captured_at: 2026-06-14T01:50:50Z
---

# Episode: Snapshot cache defeated by global rev churn — full-library re-serialization every tick

## Prior State

The rev-gated snapshot-string cache in build_snapshot_payload was assumed to skip re-serialization when nothing changed — a fast-path optimization for the push-projection tick path.

## Trigger

CPU profile of process 21680 showed 57% of samples (~1633/2856) in build_snapshot_payload → serde_json::to_string, and 14.6 GB physical footprint. Investigation of the cache logic confirmed it works correctly — the cache is defeated because rev.fetch_add is called from many handlers on essentially every actor tick.

## Decision

Diagnostic conclusion: the rev-gated cache is structurally sound but defeated by rev churn — rev bumps from comments_handler, feed_fetch, knowledge, agent_note_handler, etc. mean the cache almost never hits. The leaf bottleneck (format_escaped_str on huge payloads) is serde doing its job; the real fix is reducing what gets serialized per tick (delta snapshots, per-podcast caching, or structural change to push individual PodcastSummary updates instead of the full PodcastUpdate envelope).

## Consequences

- 14.6 GB physical footprint is likely from accumulated allocations from repeated full-library serializations
- Any per-field change in any podcast/episode bumps rev, invalidating the entire snapshot cache
- The fix must be structural (delta/individual projections) rather than a cache-tuning tweak
- The push-projection path reuses the same full-library serialization as the old pull path — both share the defect

## Open Tail

- No specific fix was adopted this session — three options were identified (delta snapshots, cache serialized form per-podcast, push individual updates) but not yet implemented

## Evidence

- transcript lines 30-57
- transcript lines 122-161
- transcript lines 163-171

