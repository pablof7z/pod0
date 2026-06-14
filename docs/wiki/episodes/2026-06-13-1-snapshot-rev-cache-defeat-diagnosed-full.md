---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-serialization
  - rev-gated-cache
  - podcast-update
supersedes:
  - 2026-06-13-1-snapshot-serialization-hot-path-rev-gated
related_claims: []
source_lines:
  - 30-57
  - 122-161
captured_at: 2026-06-13T20:08:59Z
---

# Episode: Snapshot rev-cache defeat diagnosed — full-library serialization every tick

## Prior State

The snapshot path has a rev-gated cache (snapshot_cache + rev counter) intended to skip re-serialization when nothing changed. The assumption was that this cache makes the push-frame snapshot path cheap.

## Trigger

Profiling process 21680 showed 57% of samples (~1633/2856) in build_snapshot_payload → serde_json::to_string, and 14.6 GB physical footprint. The rev-gated cache exists but is defeated because rev (fetch_add on every command dispatch) bumps on essentially every actor tick, causing full Vec<PodcastSummary> × Vec<EpisodeSummary> re-serialization on every tick.

## Decision

Root cause identified: rev bumps too granularly, defeating the cache. Three fix directions surfaced but not yet chosen: (1) delta snapshots — only serialize changed podcasts/episodes, (2) per-podcast cached serialized form — only re-serialize when a podcast actually changes, (3) structural change — push individual PodcastSummary updates rather than the full PodcastUpdate envelope per tick.

## Consequences

- The leaf hot spot (format_escaped_str / serde string escaping) is not the fix target — reducing what gets serialized per tick is
- 14.6 GB physical footprint likely from accumulating allocations from repeated full-library serializations
- Any fix must preserve the push-frame semantics while reducing per-tick serialization volume
- The existing perf_ffi_snapshot_transport.md doc already identified this as a known perf issue

## Open Tail

- Which of the three fix directions to implement has not been decided
- Whether rev can be made more granular (per-podcast vs global) without breaking consistency

## Evidence

- transcript lines 30-57
- transcript lines 122-161

