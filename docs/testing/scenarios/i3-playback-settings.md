# Scenario I3: Playback settings

## Goal
Validate the Playback settings: default speed, skip intervals, headphone gestures,
auto-mark-played, auto-play-next, auto-skip-ads.

## Prerequisites
- App past onboarding.

## Steps
1. Settings → Listening → **Player** (PlaybackSettingsView, title "Playback").
   *Screenshot.*
2. Change **Default Speed** (0.8 / 1.0 / 1.2 / 1.5 / 2.0). **Expected:** Selection
   persists; footer says it applies to new episodes. *Screenshot.*
3. Change **Skip Back** and **Skip Forward** (10/15/30/45/60/75/90 sec). **Expected:**
   Persist; cross-check the player buttons reflect the new intervals (D2). *Screenshot.*
4. Set **Double-Tap** / **Triple-Tap** headphone gesture actions. **Expected:**
   Persist. *Screenshot.*
5. Toggle **Auto-mark played at end**, **Auto-play next from queue**, and
   **Auto-skip ads**. **Expected:** Each toggle persists with the documented footer.
   *Screenshot.*

## Acceptance Criteria
- Every picker and toggle persists across navigation and relaunch.
- Default speed applies to new episodes (per-session override is separate; D4).
- Skip intervals propagate to the in-app and lock-screen skip controls.
- Auto-mark / auto-play-next / auto-skip-ads behaviors match their footers.

## Known Issues / Watch Points
- Auto-play-next interacts with the end-of-episode sleep timer (timer stops first).
- Auto-skip-ads quality varies; ads are still flagged in chapters even when off.
- Settings are kernel-owned — a setting that resets on relaunch is a persistence bug.

## Notes
