---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - maybe-auto-advance
  - playback-staging
  - lock-poison-divergence
supersedes:
  - 2026-06-14-3-auto-advance-must-stage-the-episode
related_claims: []
source_lines:
  - 10950-10953
  - 11007-11024
  - 11057-11075
captured_at: 2026-06-14T04:04:34Z
---

# Episode: Auto-advance must stage-or-bail — no playback without a persistable record

## Prior State

`maybe_auto_advance` staged the next episode under `if let Ok(mut actor) = handle.state.playback.player.lock()` but then unconditionally dispatched `Load`+`Play` at lines 292-296. On a poisoned lock, staging was silently skipped while playback still started — the same bug class as a previously-fixed lock-screen-play divergence (no staged record → position never persists, episode never marked played).

## Trigger

Cycle-15 planner identified the latent correctness bug: the staging guard and dispatch sites are not atomically coupled, so a poisoned lock creates a divergence where audio plays with no tracking record.

## Decision

Convert `if let Ok` staging guard to a `match` with `Err(_) => return` before the dispatch sites, so stage-and-dispatch are one atomic decision — if staging fails, the function bails entirely. Also added D6 null-app guards on `dispatch_audio_cmd`/`dispatch_download_cmd` matching existing patterns.

## Consequences

- No playback can start without a staged record — position writeback and mark-played will always have an episode_id to correlate
- Matches the pattern of the earlier lock-screen-play fix
- Three behavioral tests lock in the happy-path, empty-queue bail, and disabled-flag-gate scenarios

## Open Tail

*(none)*

## Evidence

- transcript lines 10950-10953
- transcript lines 11007-11024
- transcript lines 11057-11075

