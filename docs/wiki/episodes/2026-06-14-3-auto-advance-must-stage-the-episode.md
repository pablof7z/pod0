---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - auto-advance
  - playback-staging
  - lock-poison-safety
supersedes:
  - 2026-06-14-4-auto-advance-must-stage-the-episode
related_claims: []
source_lines:
  - 10951-10952
  - 11012-11024
  - 11057-11074
captured_at: 2026-06-14T03:48:31Z
---

# Episode: Auto-advance must stage the episode record or not start playback (lock-poison divergence)

## Prior State

maybe_auto_advance staged the next episode under if let Ok(mut actor) = lock() but then unconditionally dispatched Load+Play; on a poisoned lock, staging was silently skipped while playback still started — position never persisted and episode was never marked played

## Trigger

Session planner identified the latent correctness bug — same class as an already-fixed lock-screen-play divergence: stage-and-dispatch must be one atomic decision

## Decision

Convert the if-let-ok staging guard to a match with Err(_) => return before the dispatch sites; if staging fails, the function bails entirely — no playback without a staged record. Also added D6 null-app guards on dispatch_audio_cmd/dispatch_download_cmd consistent with existing PodcastHostOpHandler pattern

## Consequences

- Stage-and-dispatch are now one atomic decision — no divergence possible on poisoned lock
- D6 null-app guards added for consistency with existing dispatch_audio pattern
- Three behavioral tests cover happy path, empty-queue bail, and disabled auto_play_next flag

## Open Tail

*(none)*

## Evidence

- transcript lines 10951-10952
- transcript lines 11012-11024
- transcript lines 11057-11074

