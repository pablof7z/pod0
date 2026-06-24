# Scenario D7: Queue / Up Next

## Goal
Validate adding episodes to the queue, reordering (Move to top), removing, clearing,
and auto-play-next.

## Prerequisites
- App past onboarding with ≥3 episodes available.

## Steps
1. From an episode row, swipe leading → **Add to Queue** (or context menu "Add to
   Queue"). **Expected:** Episode joins Up Next. Repeat for 2–3 episodes. *Screenshot.*
2. Open the queue (player More menu → "Up Next" / `player-queue-chip`). **Expected:**
   PlayerQueueSheet titled "Up Next" listing the queued items
   (`queue-row-<id>`), with a total runtime footer. *Screenshot.*
3. Long-press a queue row → **Move to top** (accessibility action "Move to top").
   **Expected:** That item moves to position 1. *Screenshot.*
4. Swipe a queue row → **Remove**. **Expected:** Item removed. *Screenshot.*
5. Tap **Clear queue** (footer, destructive). **Expected:** Confirmation, then empty
   state "Nothing queued". *Screenshot.*
6. (Auto-play) With "Auto-play next from queue" enabled and ≥1 queued item, let an
   episode reach its end. **Expected:** Next queued episode begins. *Screenshot.*

## Acceptance Criteria
- Episodes can be queued from rows; the queue lists them with show/title/duration.
- Move to top reorders correctly; Remove deletes a single item; Clear empties it.
- The runtime footer sums queued durations.
- Auto-play-next starts the next queued item at end of episode (per setting).

## Known Issues / Watch Points
- Queue rows: `queue-row-<item.id>`; context-menu items lack identifiers — use labels.
- End-of-episode sleep timer mode stops BEFORE auto-play-next fires (see D5).

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, 11:20 UTC**

**Step-by-step observations:**
- Step 1: BLOCKED — Queue is empty; unable to add episodes to queue. The queue sheet displays "Nothing queued" state, indicating the queue was either cleared or the app state was reset since the previous test session (2026-06-24, 3:31 UTC) when 3 episodes were successfully queued. Attempted to add episodes by:
  - Navigating to player More menu → "Up Next" ✓ (queue sheet opens successfully)
  - Trying to access library episode rows to test "Add to Queue" context menu
  - Issue: App navigation blocked by always-open player overlay; unable to reach episode rows/library list view to initiate queue actions

- Step 2: Queue sheet behavior verified (from previous session):
  ✓ PlayerQueueSheet titled "Up Next" opens from player More menu
  ✓ Empty state shows "Nothing queued" message with helpful text
  ✓ UI structure appears correct
  ⚠ Current test unable to proceed — no queued episodes to validate functionality

- Steps 3-6: NOT COMPLETED (blocker in step 1)
  - "Move to top" action: Cannot test without queued episodes
  - "Remove" swipe action: Cannot test without queued episodes
  - "Clear queue" button: Visible in UI but cannot test without queued episodes
  - Auto-play-next: Cannot test without queued episodes

**Acceptance Criteria Status:**
- ✗ Episodes cannot be queued (queue empty, unable to add episodes via current navigation)
- ⚠ Move to top: Not tested (prerequisites not met)
- ⚠ Remove: Not tested (prerequisites not met)
- ⚠ Clear: Not tested (prerequisites not met)
- ? Runtime footer: Not testable without queued episodes
- ? Auto-play-next: Not tested (prerequisites not met)

**Blocker Analysis:**
The primary blocker is the empty queue combined with difficulty accessing episode rows from the home view. The player mini-player overlay dominates the screen, and tapping on show names or episodes leads back to the player rather than the show/episode detail view. To proceed, either:
1. Manually add episodes to queue before the scenario test, OR
2. Fix navigation so episode rows are accessible with swipe/context menu actions
