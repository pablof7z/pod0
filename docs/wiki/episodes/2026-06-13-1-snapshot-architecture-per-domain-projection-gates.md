---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - snapshot-serialization
  - projection-gates
  - actor-tick-perf
supersedes:
  - 2026-06-13-1-snapshot-rev-cache-defeat-diagnosed-full
  - 2026-06-12-1-per-domain-delta-transport-replaces-full
related_claims: []
source_lines:
  - 30-57
  - 108-120
  - 134-154
  - 161-161
captured_at: 2026-06-13T22:49:46Z
---

# Episode: Snapshot architecture: per-domain projection gates replace whole-library rebuild

## Prior State

build_snapshot_payload re-serializes the entire podcast library (all podcasts × all episodes) to JSON on every actor tick via emit_now. A rev-gated snapshot-string cache exists but is defeated because rev bumps on essentially every command dispatch, causing cache misses on every tick.

## Trigger

Profiling process 21680 showed 57% of CPU (~1633/2856 samples) in build_snapshot_payload → serde_json::to_string, plus 14.6GB physical footprint from accumulated allocations. Investigation confirmed the rev cache is correct but rev bumps on every tick invalidate it, making the cache useless.

## Decision

Kill the 1 Hz whole-library rebuild. Adopt per-domain projection gates so only changed domains are serialized per tick. The push projection under nmp_app_register replaces the bespoke nmp_app_podcast_snapshot pull symbol and the shell's 500ms poll (a D8 violation / reborn deprecated chirp_snapshot pattern).

## Consequences

- Eliminates the dominant CPU bottleneck (57% in serialization)
- Projection-gated updates are now the canonical snapshot-output seam
- The shell no longer polls; it receives pushed deltas
- Per-domain gating means unchanged podcasts/episodes incur zero serialization cost

## Open Tail

- Need to verify the per-domain projection implementation covers all snapshot consumers
- The 14.6GB footprint may linger until the old full-serialization allocation pattern is fully removed

## Evidence

- transcript lines 30-57
- transcript lines 108-120
- transcript lines 134-154
- transcript lines 161-161

