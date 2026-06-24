# Scenario C4: Mark episode played / unplayed

## Goal
Validate marking an episode played and unplayed via the episode-row context menu,
swipe action, and episode-detail / player more menu — and that the state badge and
unplayed counts update.

## Prerequisites
- App past onboarding with a subscribed show containing an unplayed episode.

## Steps
1. In a show's episode list, note an unplayed episode (red-dot badge, bold title).
   *Screenshot.*
2. Long-press the row. **Expected:** Context menu with "Mark as played" (among
   others). Tap it. **Expected:** Title dims, badge flips to checkmark; the show's
   unplayed count decrements. *Screenshot.*
3. Long-press again → **Mark as unplayed**. **Expected:** Reverts to unplayed badge;
   count increments. *Screenshot.*
4. (Alternate) Open the episode detail → toolbar (…) → "Mark as played" /
   "Mark as unplayed". **Expected:** Same toggle behavior. *Screenshot.*
5. (Alternate) During playback, open the player More menu → "Mark as played".
   **Expected:** Toggles and may stop/advance per settings. *Screenshot.*

## Acceptance Criteria
- Mark-as-played dims the row, flips the badge to a checkmark, and decrements the
  show's unplayed count.
- Mark-as-unplayed reverses all of the above.
- The same toggle is reachable from row menu, detail menu, and player menu.

## Known Issues / Watch Points
- With "Auto-mark played at end" enabled, reaching the end auto-marks — verify the
  manual toggle still works independently.
- Unplayed-count update is reactive; a stale count after toggling is a projection bug.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, 10:47-10:49 UTC**

Unable to complete scenario testing due to prerequisite condition not met.

**Issues encountered:**
- App installed and launched successfully from fresh state (past onboarding UI)
- No subscribed shows in the app (Subscriptions view showed "No subscriptions yet")
- Attempted to add a podcast during onboarding, but the show did not persist
- App requires a subscribed show with unplayed episodes to proceed with the test
- Setting up test data via the Library/search functionality exceeded the 4-minute time budget

**Test data setup status:**
- Step 1 (Locate unplayed episode): BLOCKED — No shows to browse
- Step 2-5: NOT TESTED (dependent on Step 1)

**Acceptance criteria:** NOT TESTED due to missing prerequisite

**Recommendation:** 
- Pre-populate the app with test data (hardcoded show + unplayed episodes in the test fixture)
- Or use a faster onboarding path that properly adds a show during initial setup
- Scenario requires a properly initialized app state with at least one subscribed show containing multiple unplayed episodes
