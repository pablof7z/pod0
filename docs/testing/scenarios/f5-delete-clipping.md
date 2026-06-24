# Scenario F5: Delete a clipping

## Goal
Validate deleting a clipping via the context menu and the swipe action, and that it
is removed from the list.

## Prerequisites
- App past onboarding with ≥2 clips (so the list isn't empty after deletion).

## Steps
1. Open the **Clippings** tab. Note the clip count. *Screenshot.*
2. Swipe a clip card trailing → **Delete** (destructive). **Expected:** The clip is
   removed; count decrements. *Screenshot.*
3. Long-press another clip → **Delete**. **Expected:** Removed. *Screenshot.*
4. Relaunch the app (or navigate away and back). **Expected:** Deleted clips do NOT
   reappear (deletion persisted to the kernel clips sidecar). *Screenshot.*

## Acceptance Criteria
- Both swipe-to-delete and context-menu delete remove the clip.
- The list updates immediately and the count decrements.
- Deletion persists across relaunch.

## Known Issues / Watch Points
- Clips are stored in a kernel-owned `clips.json` sidecar; a delete that reappears
  after relaunch indicates the sidecar wasn't updated.
- If the deleted clip was published to Nostr, local deletion does not necessarily
  retract the relay event — note this distinction.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24 at 05:03 UTC**

Prerequisite not met: The clippings list contains only 1 clip, but the scenario requires ≥2 clips to test deletion without emptying the list.

**Observations:**
- Step 1: Opened Clippings tab successfully. Navigated via sidebar menu.
- Current state: 1 clip visible in "TODAY" section
  - Podcast: The Daily
  - Clip text: "Clip from R.F.K. Jr.'s Newest Mission: Getting Us Off Antidepressants"
  - Duration: 8:29 → 9:29 (1:00)
  - Posted: 1h ago
- Scrolled down to check for additional clips (older dates) - none found.

**Blocker:** Cannot proceed with test because deleting the single clip would result in an empty list, which violates the prerequisite "so the list isn't empty after deletion."

**Recommendation:** Populate the clippings list with at least 2 clips before retesting this scenario.
