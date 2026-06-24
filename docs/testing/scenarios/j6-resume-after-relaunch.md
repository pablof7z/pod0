# Scenario J6: Resume playback after app relaunch

## Goal
Validate that playback position and the "now playing"/continue-listening state
survive killing and relaunching the app.

## Prerequisites
- App past onboarding, an episode played to a non-trivial position (e.g., several
  minutes in), then paused.

## Steps
1. Play an episode to ~5:00 and pause. Note the exact position. *Screenshot.*
2. Fully terminate the app (swipe-kill in the app switcher). *Screenshot.*
3. Relaunch the app (use `--UITestSeedRelaunch` if seeding, to preserve kernel
   state). **Expected:** The app reopens to the main UI (not onboarding). *Screenshot.*
4. Open Home → Continue Listening. **Expected:** The episode appears with the saved
   progress ("Xh Ym left" / resume). *Screenshot.*
5. Tap it / open the player and resume. **Expected:** Playback resumes at (or very
   near) the saved position, not from 0. *Screenshot.*

## Acceptance Criteria
- After relaunch, the episode's saved position is preserved (Continue Listening shows
  it; resuming starts at the saved position).
- Onboarding does NOT reappear on relaunch.
- The mini-player / now-playing state restores appropriately.

## Known Issues / Watch Points
- MEMORY/BACKLOG: `testP0_04_ResumeReopenByTitle` was a known flaky/blocking UI test;
  PR #497 hardened reopen flows. Watch for the position resetting to 0 or the wrong
  episode resuming.
- Position persistence is debounced — the saved position may be a few seconds behind
  the exact pause point; a large discrepancy is a bug.
- `--UITestSeed` (without Relaunch) WIPES SQLite and reseeds; use `--UITestSeedRelaunch`
  to preserve kernel state across the relaunch.

## Notes
