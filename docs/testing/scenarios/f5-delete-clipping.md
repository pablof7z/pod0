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

**Result: PASS**
**Tested: 2026-06-24 at 08:48-08:49 UTC**

All acceptance criteria met. Both deletion methods (swipe-to-delete and long-press context menu) work correctly, the list updates immediately, and deletions persist across app relaunch.

**Observations:**
- Step 1: Opened Clippings tab successfully via sidebar menu.
  - Initial state: 2 clips in "TODAY" section
  - Clip 1: This American Life - 137: The Book That Changed You... (0:00 → 0:49, 49s, 19m ago)
  - Clip 2: This American Life - 137: The Book That Changed You... (0:30 → 1:30, 1:00, 21m ago)

- Step 2: Swipe-to-delete first clip
  - Swiped the first clip card left
  - Red "Delete" button appeared on the right
  - Tapped the Delete button
  - Clip was immediately removed from the list
  - Count decremented from 2 to 1

- Step 3: Long-press context menu delete
  - Long-pressed the remaining clip (500ms duration)
  - Context menu appeared with options: Play Clip, Share, Open Episode, Delete (red)
  - Tapped Delete button
  - Clip was immediately removed
  - List now shows "No Clippings Yet" empty state
  - Count decremented from 1 to 0

- Step 4: Relaunch persistence check
  - Stopped and relaunched the app
  - Navigated to Clippings tab
  - App still shows "No Clippings Yet"
  - Deletions persisted to kernel clips.json sidecar

**Acceptance Criteria Met:**
- ✓ Both swipe-to-delete and context-menu delete remove clips
- ✓ List updates immediately and count decrements
- ✓ Deletion persists across app relaunch
