# Scenario D2: Skip forward / back

## Goal
Validate the transport skip buttons jump by the configured interval (default 15s),
and that long-press jumps to the previous/next chapter.

## Prerequisites
- App past onboarding, an episode playing. For chapter long-press, use an episode
  with chapters (`--UITestSeed` seeds chapters for ep1).

## Steps
1. Start playback and note the current time. *Screenshot.*
2. Tap **skip forward** (`player-skip-forward`, label "Skip forward 15 seconds").
   **Expected:** Position jumps +15s (or the configured interval). *Screenshot.*
3. Tap **skip backward** (`player-skip-backward`, label "Skip back 15 seconds").
   **Expected:** Position jumps -15s. *Screenshot.*
4. Long-press (≥0.45s) **skip forward**. **Expected:** Jumps to the NEXT chapter
   start ("Next chapter"). *Screenshot.*
5. Long-press **skip backward**. **Expected:** Jumps to the PREVIOUS chapter start
   ("Previous chapter"). *Screenshot.*
6. (Cross-check) Change the skip interval in Settings → Playback to 30s, return and
   verify the skip button glyph/label/behavior reflects 30s. *Screenshot.*

## Acceptance Criteria
- Tap forward/back moves position by exactly the configured interval.
- The SF Symbol matches the interval (gobackward.15 / goforward.15 for defaults).
- Long-press moves to adjacent chapter boundaries (only when chapters exist).
- Changing the interval in Settings updates the button.

## Known Issues / Watch Points
- Off-grid intervals (not in 10/15/30/45/60/75/90) fall back to a bare glyph.
- Long-press requires ≥0.45s; a too-short press just performs a normal skip.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24 ~11:02 AM**

**Blocker:** Test data not available. The app was rebuilt with `--UITestSeed` flag expecting it to populate test episodes and chapters, but the app started with no subscribed shows or episodes. The prerequisite states "an episode playing" is required, but the test data seeding mechanism did not populate the library. Without an episode in playback, the skip forward/back buttons cannot be tested.

Steps attempted:
1. Built and ran app with `--UITestSeed` launch argument
2. Navigated through home, library, and settings screens
3. Library showed "No episodes yet" - no test data was seeded

**Expected resolution:** Either:
- Confirm that `--UITestSeed` flag is the correct way to seed test data, or
- Document alternative method to populate test episodes for testing, or
- Pre-populate a fixture/database with test episodes before running scenario

**Acceptance criteria met: NO** - Cannot test skip buttons without a playing episode
