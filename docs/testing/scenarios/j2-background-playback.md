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
