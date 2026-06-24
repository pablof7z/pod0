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

**Result: BLOCKED**
**Tested: 2026-06-24 01:12 UTC**

Technical blocker encountered:
- UI automation `snapshot_ui` command consistently times out (30s daemon timeout) after 4+ attempts
- Daemon is running and responsive (status check confirms PID 73818)
- Other commands work (screenshots, daemon status, launch-app)
- Restarted daemon with no improvement
- Root cause: XcodeBuildMCP runtime UI semantic snapshot generation fails to complete
- Impact: Cannot obtain elementRef values needed to tap UI elements (tap command requires elementRef from snapshot_ui)
- Workaround attempted: None available without snapshot_ui or manual coordinates (which are not supported by xcodebuildmcp UI automation)

Unable to proceed with scenario execution. Scenario cannot be tested until snapshot_ui is functional.
