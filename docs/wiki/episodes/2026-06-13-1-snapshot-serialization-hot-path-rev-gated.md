---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-serialization
  - podcast-update-rev-cache
  - build-snapshot-payload
supersedes:
  - 2026-06-13-1-snapshot-rev-cache-defeated-by-per
related_claims: []
source_lines:
  - 30-57
  - 122-161
captured_at: 2026-06-13T19:50:41Z
---

# Episode: Snapshot serialization hot path: rev-gated cache defeated by per-tick rev bumps

## Prior State

The rev-gated snapshot-string cache in build_snapshot_payload was believed to make re-serialization cheap — an unchanged rev would clone the cached JSON string, skipping full-library serialization.

## Trigger

Process sample (pid 21680) showed 57% of samples (~1633/2856) in build_snapshot_payload → serde_json::to_string, plus 14.6 GB physical footprint. Inspection of the rev-bump sites (comments_handler, feed_fetch, knowledge, agent_note_handler, etc.) revealed that rev.fetch_add(1) fires on essentially every actor command dispatch, invalidating the cache on every tick.

## Decision

Root cause identified: the rev counter bumps on every actor command, so the 'fast path' cache hit never fires — the entire library (all podcasts × all episodes) is re-serialized to JSON on every tick. The leaf bottleneck (format_escaped_str / serde string escaping) is a symptom of the massive payload, not the fix target. Three architectural remedies identified: (1) delta snapshots — only serialize changed podcasts/episodes, (2) structural change — push individual PodcastSummary updates rather than the full PodcastUpdate envelope, (3) reduce rev granularity so the cache can actually hit. No single remedy was picked in this session; the diagnosis itself reframes the solution space.

## Consequences

- Any future snapshot perf fix must address rev granularity or payload scope — merely optimizing serde is insufficient
- The 14.6 GB footprint is likely from accumulated allocations of repeated full-library serializations, not a separate leak
- The push-frame projection (NmpApp::register_snapshot_projection) calls build_snapshot_payload on every tick, so both pull and push paths are affected

## Open Tail

- Which of the three remedies (delta snapshots, structural push, reduced rev granularity) to implement remains unchosen
- No session-level decision was made to act on this finding — it was diagnosed then the session pivoted to PR reviews

## Evidence

- transcript lines 30-57
- transcript lines 122-161

