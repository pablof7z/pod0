---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - auto-advance-lock-divergence
  - playback-staging-atomicity
supersedes:
  - 2026-06-14-5-auto-advance-must-stage-the-episode
related_claims: []
source_lines:
  - 10949-10952
  - 11007-11024
  - 11057-11075
captured_at: 2026-06-14T02:13:47Z
---

# Episode: Auto-advance must stage the episode record or not start playback — lock-poison divergence

## Prior State

maybe_auto_advance staged the next episode under if let Ok(actor) = lock() but unconditionally dispatched Load+Play; on a poisoned lock, playback started with no staged record, causing position to never persist and episode to never be marked played — same bug class as a prior lock-screen-play fix

## Trigger

Planner identified the latent correctness bug: stage-under-lock vs unconditional dispatch is a lock-divergence pattern matching a previously-fixed class

## Decision

Convert the if-let-Ok staging guard to a match with Err(_) => return before the dispatch sites; stage-and-dispatch are now one atomic decision — if staging fails, the function bails entirely

## Consequences

- No playback can start without a staged record — the divergence class is closed
- D6 null-app guards added to dispatch_audio_cmd/dispatch_download_cmd for test correctness
- Three behavioral tests added (happy path, empty-queue bail, flag-gate)
- Validated by the new headless e2e CI gate on merge

## Open Tail

*(none)*

## Evidence

- transcript lines 10949-10952
- transcript lines 11007-11024
- transcript lines 11057-11075

