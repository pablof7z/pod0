---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-perf
  - projection-rebuild
  - domain-revs
supersedes:
  - 2026-06-13-1-kill-1hz-whole-library-snapshot-rebuild
  - 2026-06-13-2-deprioritize-podcast-misc-domain-split-hot
related_claims: []
source_lines:
  - 32-57
  - 122-161
  - 8478-8486
captured_at: 2026-06-13T05:46:50Z
---

# Episode: Per-domain projection gates kill snapshot hot path; misc split explicitly deprioritized

## Prior State

Every actor-tick command dispatch triggered `emit_now` → `build_snapshot_payload`, which re-serialized the entire library (all podcasts × all episodes) to JSON. A rev-gated snapshot-string cache existed but was defeated because `rev` bumped on essentially every command, causing 57% of CPU samples to land in `serde_json::to_string` and a 14.6 GB physical footprint. The next planned optimization was to further split `podcast.misc` into per-subdomain projections.

## Trigger

CPU profile of process 21680 showed 1,633/2,856 samples (~57%) in `build_snapshot_payload` → `serde_json::to_string`. Investigation of the rev-gated cache (lines 134-154) revealed the cache was correct but `rev` was bumping on every tick via `fetch_add` calls scattered across handlers, invalidating it. Subsequent cycle-10 planner verification confirmed the misc rev only advances on low-frequency wiki/knowledge/picks edits, while high-frequency mutators (agent-chat, voice, clips, comments) use global `signal.bump()` and never advance a domain rev.

## Decision

Adopted slice-local domain payload builders gated on per-domain revs (#425), so only domains whose rev actually changed rebuild their snapshot — killing the 1 Hz whole-library rebuild. Explicitly deprioritized further splitting of `podcast.misc` into `podcast.agent`/`wiki`/`clips` because #425 already eliminated the hot path and the remaining rebuilds only fire on manual wiki/pick edits.

## Consequences

- The 57%-CPU serialization hot path is eliminated; only changed domains re-serialize.
- Further `podcast.misc` sub-splits would be pure hygiene requiring matching iOS+Android frame structs + golden/real-bump gates for near-zero perf gain — not worth a cycle.
- Latent correctness question remains: agent-chat/voice deltas never advance a domain rev, so the push frame may never carry their deltas (pull path remains the hydration fallback; shells re-pull on global-rev frames).

## Open Tail

- Verify that agent-chat/voice token deltas are visible via the pull path when they don't advance a domain rev.

## Evidence

- transcript lines 32-57
- transcript lines 122-161
- transcript lines 8478-8486

