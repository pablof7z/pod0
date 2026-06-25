# Scenario J2: App backgrounded during playback

## Goal
Validate that audio continues when the app is backgrounded and that state is intact
on return to foreground.

## Prerequisites
- App past onboarding, an episode playing.

## Steps
1. Start playback; note the position. *Screenshot.*
2. Send the app to the background (Home / app switcher). **Expected:** Audio
   continues playing. *Screenshot (Control Center Now Playing while backgrounded).* 
3. Wait ~30s. Reopen the app. **Expected:** Playback is still going; the in-app
   position reflects the elapsed background time accurately. *Screenshot.*
4. Pause from Control Center while backgrounded, then foreground. **Expected:** The
   in-app player shows paused. *Screenshot.*

## Acceptance Criteria
- Audio continues during background.
- On return, the player position is accurate (advanced by the background elapsed time).
- Background remote control (pause) is reflected in-app on foreground.

## Known Issues / Watch Points
- Position updates are debounced (MEMORY/BACKLOG references
  testPositionUpdatesAreDebounced) — a small lag in the displayed position on
  foreground is acceptable; a large drift is not.
- Watch for audio-session interruptions (e.g., another app) being handled.

## Notes

**Result: FAIL**
**Tested: 2026-06-24 10:36 (approx)**

Step 1: Started playback successfully
- Tapped play button on episode "137: The Book That Changed Your Life"
- Mini player showed pause button (II) and position 0:08
- Playback was active ✓

Step 2: Sent app to background
- Pressed home button successfully
- App backgrounded and home screen displayed
- Attempted to open Control Center (swipe-from-top-edge gesture) but it did not appear on sim
- Note: Control Center may not be visible on simulator for testing audio playback

Step 3: Waited and reopened app
- Relaunched app after brief wait
- **ISSUE DETECTED**: Mini player shows position 0:00 (RESET, not advanced)
- Play button visible (not pause), indicating playback is stopped
- Expected: Position should have advanced by ~30+ seconds during background time
- Opened player detail sheet - confirmed position at 0:00

Step 4: NOT EXECUTED (blocked by Step 3 failure)

**Acceptance Criteria Status:**
- ❌ Audio continues during background - FAIL (stopped, position reset)
- ❌ On return, player position accurate - FAIL (reset to 0:00, not advanced)
- ❓ Background remote control - BLOCKED (depends on Step 3 passing)

**Screenshots:**
1. Pre-background playback: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_5879ed30-68d9-40a0-b47c-10230a117e7b.jpg (0:08 position, pause button)
2. App backgrounded (home screen): /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_9b89a32e-0cab-43e0-b2f8-4b9fb734ba16.jpg
3. Post-reopen (reset position): /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_6028ab61-e708-4126-aab6-76a57305f06c.jpg (0:00, play button)
4. Player detail sheet: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_23a96234-133a-4df7-84d8-42d106d21574.jpg

**Root Cause Assessment:**
Background audio playback is not implemented or not functioning. The playback session appears to suspend/stop when the app is backgrounded rather than continuing. This is a fundamental feature gap for a podcast player app.
