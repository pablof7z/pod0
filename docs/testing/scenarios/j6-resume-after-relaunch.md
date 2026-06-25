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

**Result: FAIL**
**Tested: 2026-06-24, ~11:14 AM**

Playback position is NOT persisted across app relaunch.

**Step-by-step observations:**

- Step 1: Played episode "137: The Book That Changed Your Life" (5m duration) to position 1:00 and paused. Screenshot taken showing player at 1:00 / -4:00 remaining.
- Step 2: Terminated app using `xcodebuildmcp simulator stop --bundle-id io.f7z.podcast`.
- Step 3: Relaunched app using `xcodebuildmcp simulator launch-app --bundle-id io.f7z.podcast`. App reopened to Home view (not onboarding). ✓ PASS
- Step 4: Checked Home → Continue Listening. The mini-player shows the episode title but position appears reset to 0:08 in mini-player bar.
- Step 5: Opened full player by tapping mini-player. Player now shows 0:00 / -1:00, indicating the playback position has been completely reset.

**Critical Finding:**
The playback position of 1:00 set in Step 1 was NOT preserved. After relaunch, the player shows 0:00, contradicting the scenario requirement of resuming at the saved position.

**Acceptance Criteria Status:**
- ✗ FAIL: After relaunch, episode's saved position is NOT preserved — it reset to 0:00
- ✓ PASS: Onboarding does NOT reappear on relaunch
- ✗ FAIL: The mini-player / now-playing state does NOT restore appropriately (shows 0:00 instead of 1:00)

**Known Issue Match:**
This matches the known issue mentioned in the Watch Points: "Watch for the position resetting to 0" — which is exactly what occurred in this test.
