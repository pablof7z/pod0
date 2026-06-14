---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-perf
  - build-snapshot-payload
  - rev-cache-defeat
supersedes: []
related_claims: []
source_lines:
  - 1-161
captured_at: 2026-06-13T23:44:31Z
---

# Episode: Snapshot cache defeated by global rev — full-library re-serialization on every tick

## Prior State

The rev-gated snapshot-string cache in build_snapshot_payload was believed to be effective — an unchanged rev would short-circuit serialization.

## Trigger

sample profiling of process 21680 showed ~57% of CPU time in build_snapshot_payload → serde_json::to_string and a 14.6GB physical footprint; investigation revealed rev fetch_add is called from many handlers (comments, feed_fetch, knowledge, agent_note, etc.) on essentially every actor tick, defeating the cache entirely.

## Decision

Root cause identified: the cache is structurally correct but rev bumps on every command dispatch, so the full library is re-serialized every tick. No fix was implemented this session, but the diagnosis reframes the problem — the fix must be structural (delta snapshots, per-podcast serialization caching, or pushing individual PodcastSummary updates rather than the full PodcastUpdate envelope on every tick).

## Consequences

- The perceived 'cache' is a no-op in practice; any perf work must target reducing what gets serialized per tick, not just adding caching
- 14.6GB physical footprint is likely from accumulated allocations from repeated full-library serializations
- The string-escaping leaf bottleneck (format_escaped_str) is a symptom, not the cause

## Open Tail

- Which structural fix to adopt (delta snapshots vs per-podcast cache vs structural push change) is unresolved
- Memory footprint from accumulated allocations needs separate investigation

## Evidence

- transcript lines 1-161

