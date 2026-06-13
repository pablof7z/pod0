---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-serialization-perf
  - build-snapshot-payload
  - podcast-update-rev
supersedes:
  - 2026-06-13-1-per-domain-projection-gates-kill-snapshot
related_claims: []
source_lines:
  - 30-57
  - 122-161
captured_at: 2026-06-13T19:33:27Z
---

# Episode: Snapshot rev-cache defeated by per-tick rev bump — full-library re-serialization every dispatch

## Prior State

The rev-gated snapshot-string cache in `build_snapshot_payload` was believed to provide a cheap fast path — skip re-serialization when rev hasn't changed.

## Trigger

CPU profile showed 57% of samples (~1633/2856) in `serde_json::to_string` via `build_snapshot_payload`, plus 14.6 GB physical footprint, despite the cache existing.

## Decision

The cache is structurally correct but the `rev` counter bumps on essentially every actor tick (comments, feed fetch, knowledge, agent notes, etc.), so the cache is always cold and full-library serialization runs on every dispatch. The leaf bottleneck is serde string escaping on a huge payload, but the real fix is reducing what gets serialized per tick.

## Consequences

- Three fix approaches identified: delta snapshots (serialize only changed podcasts/episodes), cache serialized form per-podcast (invalidate on change), or structural change (push individual PodcastSummary updates instead of full PodcastUpdate envelope on every tick)
- 14.6 GB memory footprint is likely from accumulating allocations from repeated full-library serializations
- Matches the known perf issue documented in `perf_ffi_snapshot_transport.md`

## Open Tail

- Which serialization-reduction approach to adopt (delta, per-item cache, or structural) is undecided
- No fix was implemented this session — the finding changes understanding of the perf problem

## Evidence

- transcript lines 30-57
- transcript lines 122-161

