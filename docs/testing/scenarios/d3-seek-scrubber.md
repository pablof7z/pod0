# Scenario D3: Seek via scrubber and timeline

## Goal
Validate seeking by dragging the scrubber and by tapping the timeline, with the
time labels updating correctly.

## Prerequisites
- App past onboarding, an episode playing.

## Steps
1. In the full player, locate the scrubber ("Playback scrubber" accessibility
   element; value "MM:SS / MM:SS"). *Screenshot.*
2. Drag the scrubber thumb forward ~halfway. **Expected:** Current-time label and
   remaining-time label (e.g., "-45:23") update to the new position; audio resumes
   from there. *Screenshot.*
3. Drag it backward. **Expected:** Position moves back accordingly. *Screenshot.*
4. (If timeline tap supported) Tap a point on the timeline / a clip-highlight zone.
   **Expected:** Direct seek to that point. *Screenshot.*
5. Verify the scrubber value label reads the new "current / total". *Screenshot.*

## Acceptance Criteria
- Dragging the scrubber seeks to the dragged position; both time labels update.
- A 4pt minimum drag distance suppresses incidental taps (a tiny tap should not
  jump the position erratically).
- Total duration label stays correct throughout.

## Known Issues / Watch Points
- Time display uses monospaced digits — verify no layout jitter during scrub.
- For very long episodes (>1h) the format becomes H:MM:SS (see J3).

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24 11:05**

Scenario cannot be executed because the app has no subscribed shows in the library. The prerequisite states "App past onboarding, an episode playing," but:
- App launches to Home/Library view with "Your shows live here" message
- No shows are subscribed
- No episodes available to play
- The player UI/scrubber cannot be tested without an active episode

Action: Populate library with at least one show subscription before retesting this scenario.
