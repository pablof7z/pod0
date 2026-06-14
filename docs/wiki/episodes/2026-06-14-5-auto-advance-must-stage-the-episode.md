---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - auto-advance
  - playback-lock
  - stage-dispatch-atomicity
supersedes: []
related_claims: []
source_lines:
  - 10950-11098
captured_at: 2026-06-14T02:04:38Z
---

# Episode: Auto-advance must stage the episode or not start playback — lock-poison divergence

## Prior State

maybe_auto_advance staged the next episode under if let Ok(mut actor) = lock() but then unconditionally dispatched Load+Play after the if-let block. On a poisoned lock, staging was silently skipped while playback still started — actor.episode_id stayed None, so position writeback and mark-played both silently dropped. This is the same bug class as the previously fixed lock-screen-play divergence.

## Trigger

Cycle-15 planner identified this as a verified latent correctness bug (the lock-screen-play fix pattern should apply here too).

## Decision

Convert the if-let staging guard to match with Err(_) => return BEFORE the dispatch sites, so stage-and-dispatch are one atomic decision. If staging fails, the function bails entirely — no playback without a staged record. Also added D6 null-app guards on dispatch_audio_cmd/dispatch_download_cmd (matching PodcastHostOpHandler::dispatch_audio pattern).

## Consequences

- No playback can start without a staged episode record
- Poisoned locks now cause a clean bail rather than silent position/mark-played data loss
- Three behavioral tests cover happy-path, empty-queue, and disabled-flag cases
- The headless e2e CI gate validated this change against live playback scenarios before merge

## Open Tail

*(none)*

## Evidence

- transcript lines 10950-11098

