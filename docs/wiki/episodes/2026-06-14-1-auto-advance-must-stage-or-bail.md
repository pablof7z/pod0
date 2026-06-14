---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - playback-auto-advance
  - lock-poison-divergence
supersedes:
  - 2026-06-14-2-auto-advance-must-stage-or-bail
related_claims: []
source_lines:
  - 11012-11098
captured_at: 2026-06-14T04:29:51Z
---

# Episode: Auto-advance must stage-or-bail on lock poison

## Prior State

`maybe_auto_advance` staged the next episode inside an `if let Ok(mut actor)` block but dispatched Load/Play unconditionally afterward. On a poisoned MutexGuard, staging silently skipped while dispatch still fired — playback could start without a persistable record (position writeback and mark-played would silently drop updates because they correlate by `episode_id`).

## Trigger

Cycle-15 root-cause analysis: the staging guard and dispatch sites were separate, so lock-poison bailed on staging but not on dispatch, creating a divergent control-flow path.

## Decision

Converted the `if let Ok` staging guard to a `match` with `Err(_) => return` placed BEFORE all dispatch sites. Stage-and-dispatch are now one atomic decision — if staging fails, the function bails entirely. Also added D6 null-app guards on `dispatch_audio_cmd`/`dispatch_download_cmd` matching the existing `PodcastHostOpHandler::dispatch_audio` pattern.

## Consequences

- No playback can start without a staged episode record (position writeback and mark-played now always have an `episode_id` to correlate).
- Poisoned lock now safely stops the entire auto-advance path rather than partially executing it.
- Three behavioral tests lock in: happy-path advance, empty-queue bail, disabled `auto_play_next` flag-gate.

## Open Tail

*(none)*

## Evidence

- transcript lines 11012-11098

