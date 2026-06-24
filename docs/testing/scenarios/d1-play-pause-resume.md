# Scenario D1: Play, pause, resume

## Goal
Validate starting playback, pausing, and resuming via the full player and the
mini-player, with correct play/pause icon and position behavior.

## Prerequisites
- App past onboarding with a playable episode (`--UITestSeed` seeds a downloaded
  episode for deterministic playback).

## Steps
1. Open an episode → tap **Play** (or the episode-detail Play pill). **Expected:**
   Audio starts; the full player or mini-player appears. *Screenshot.*
2. In the full player, locate the play/pause button (`player-play-pause`,
   accessibility label "Pause" while playing). **Expected:** Icon is pause.fill;
   the scrubber position advances. *Screenshot.*
3. Tap **Pause**. **Expected:** Audio stops; icon flips to play.fill (label "Play");
   position holds. *Screenshot.*
4. Tap **Play** again. **Expected:** Resumes from the held position. *Screenshot.*
5. Collapse to the mini-player (`mini-player-bar`). **Expected:** Title
   (`mini-player-title`) + play/pause (`mini-player-play-pause`) shown; tapping it
   toggles playback. *Screenshot.*
6. Tap the mini-player to expand. **Expected:** Returns to the full player. *Screenshot.*

## Acceptance Criteria
- Play starts audible playback and the position advances.
- Pause halts playback and freezes the position; Resume continues from there.
- The play/pause icon and accessibility label correctly reflect the state in BOTH
  the full player and mini-player.

## Known Issues / Watch Points
- Per MEMORY/BACKLOG, playback start had history of UI-test flakiness and Now
  Playing artwork off-main issues. If the first Play does not produce audio, retry.
- Verify there is no double mini-player or stuck spinner.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24 10:56 UTC**

Prerequisites cannot be satisfied:
- The scenario requires `--UITestSeed` command-line argument to seed a test episode into the app
- The xcodebuildmcp simulator build-and-run tool does not support passing custom command-line arguments
- Without the seed, the app's library is empty and no playable episodes are available
- Therefore the scenario cannot be executed

To resolve: Either:
1. Add support for passing launch arguments to xcodebuildmcp, or
2. Seed the test data directly via the file system before running the test, or
3. Create a test script that uses xcrun simctl to launch with arguments
