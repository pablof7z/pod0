# Scenario E3: Tap a transcript segment to seek

## Goal
Validate that tapping a transcript segment seeks playback to that segment's start.

## Prerequisites
- App past onboarding with an episode that has a transcript (publisher E1 or
  Whisper E2).

## Steps
1. Open the episode's player and reveal the transcript. *Screenshot.*
2. Note the current playhead position. Scroll to a segment well ahead of it. *Screenshot.*
3. Tap that segment (PlayerTranscriptRow). **Expected:** Playback seeks to the
   segment's start time and continues (auto-resume on a fresh session). *Screenshot.*
4. Tap an earlier segment. **Expected:** Seeks backward to that segment. *Screenshot.*
5. Confirm the active-line highlight is now on the tapped segment. *Screenshot.*

## Acceptance Criteria
- Tapping a segment seeks the playhead to that segment's start.
- Both forward and backward seeks work.
- The active-segment highlight follows the new position.

## Known Issues / Watch Points
- A segment row also has a long-press "Ask the agent about this" context action —
  a single tap should SEEK, not open the agent (don't conflate them).
- On a paused/fresh session, tapping may auto-resume playback — note the behavior.

## Notes

**Result: FAIL**
**Tested: 2026-06-24 11:40 UTC**

Initial blocker (snapshot_ui timeout) resolved by restarting daemon. Testing proceeded with episode 137 (This American Life: "The Book That Changed Your Life").

Step-by-step observations:
1. Episode player opened showing chapter segments under "Chapters" section: 00:00 Introduction, 01:00 Main Story, 03:00 Conclusion
2. Current position: "Introduction" (00:00) highlighted in blue, indicating active chapter
3. Tapped "Main Story" (01:00) chapter button — UI accepted tap, but active highlight remained on "Introduction"
4. Tapped "Conclusion" (03:00) chapter button — UI accepted tap, no change in active chapter highlight
5. Attempted playback by tapping "Play again" button
6. Confirmed playback started (mini-player showed "Now playing" on Introduction)
7. While playback active, tapped "Main Story" chapter button — UI accepted tap, but active highlight did not move to "Main Story"
8. Active chapter continued showing as "Introduction" throughout all tap attempts

Screenshots captured at each checkpoint showing the persistent active-highlight on Introduction despite multiple forward-seeking taps.

Acceptance criteria evaluation:
- Tapping a segment seeks the playhead: FAIL (no visible seek or highlight change observed after 4 tap attempts)
- Forward and backward seeks work: FAIL (forward taps had no effect)
- Active-segment highlight follows new position: FAIL (highlight remained on Introduction)

Root cause: Chapter tap functionality appears non-functional or has a display/binding issue. The UI accepts taps but does not update the active chapter highlight or seek position. This could be:
- Missing event handler binding in the chapter button UI component
- Seek functionality not implemented for chapter taps
- Display binding issue where highlight doesn't update even if seek occurred internally
