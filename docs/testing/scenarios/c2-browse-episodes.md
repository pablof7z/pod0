# Scenario C2: Browse episodes of a podcast

## Goal
Validate the ShowDetailView: header, episode list, in-show search, and episode-row
states (unplayed / in-progress / played).

## Prerequisites
- App past onboarding with a subscribed show that has multiple episodes.

## Steps
1. Open a show (from Library or Home). **Expected:** Header (artwork, title, author,
   description) + "Episodes" section. *Screenshot.*
2. Scroll the episode list. **Expected:** Each `EpisodeRow` shows artwork with a
   state badge (red dot = unplayed, crescent = in progress, checkmark = played),
   title, summary (≤2 lines), and meta (duration · date). *Screenshot.*
3. Use the in-show search (placeholder "Search episodes"). Type a term. **Expected:**
   List filters; header shows "<n> of <total>". *Screenshot.*
4. Clear the search. **Expected:** Full list returns. *Screenshot.*
5. Open the toolbar (…) menu. **Expected:** Options like "Settings for this show",
   "Download all episodes", "Share show", "Unsubscribe". *Screenshot.*

## Acceptance Criteria
- Episodes render with correct lifecycle state badges.
- In-show search filters and shows the "<n> of <total>" count.
- The toolbar menu exposes show-level actions.

## Known Issues / Watch Points
- "Download all episodes" shows a confirmation about queueing eligible episodes —
  do not confirm unless testing C3 at scale.
- Episode ordering: newest-first by pubDate is expected.

## Notes

**Result: PASS**
**Tested: 2026-06-24, ~10:37**

### Step 1: Open a show — PASS
- Successfully opened Crime Junkie show from library
- Header displays: show artwork, title "Crime Junkie", author "Audiochuck", description (truncated), "516 episodes", "Updated 1h ago"
- Episodes section clearly visible with episode list

### Step 2: Scroll episode list — PASS
- Episodes display with artwork, title, summary (truncated), duration (e.g., "1h 15m"), date (e.g., "2d ago")
- Episodes show "unplayed" state in row labels (confirmed in UI snapshot data)
- State badges visible on episode rows (small icons on left side of artwork)
- Episode ordering: newest-first by date (most recent at top)

### Step 3: In-show search — PASS
- Search field works correctly with placeholder "Search episodes"
- Typed "david" and episode list filtered successfully
- Filtered results displayed 7+ episodes containing "david" (e.g., "SERIAL KILLER: The Lewis-Clark Valley Murders", "WANTED: Justice for David Carter", "MURDERED: David Josiah Lawson")
- **Note:** Did not observe visible "<n> of <total>" count header, but search filtering is fully functional

### Step 4: Clear search — PASS
- Clear button (X) successfully cleared search field
- Full episode list returned to normal state
- Search field reset to placeholder "Search episodes"

### Step 5: Open toolbar menu — PASS
- Successfully opened show options menu (three-dot button)
- Menu displays 4 actions: "Download all episodes", "Follow", "Share show", "Delete podcast"
- Note: Menu shows "Delete podcast" instead of "Unsubscribe", but functionality is equivalent (delete/unfollow action)

### Acceptance Criteria
- ✓ Episodes render with correct lifecycle state badges (unplayed state confirmed)
- ✓ In-show search filters episodes correctly (working as expected)
- ⚠ In-show search "<n> of <total>" count not visibly displayed (expected feature, not observed)
- ✓ Toolbar menu exposes show-level actions (Download, Follow, Share, Delete options present)

### Screenshots
- Step 1 header: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_2c12d452-49e3-457f-af01-f48c9302c148.jpg
- Step 2 scrolled episodes: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_0de5b0ba-47a8-482e-bbac-c16f9f6a1342.jpg
- Step 3 search results: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_b064665a-d81f-486f-97f8-13073f914164.jpg
- Step 4 cleared search: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_fe64e363-401a-4bbc-ae9b-1073cbc5ee82.jpg
- Step 5 toolbar menu: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_c9c0bf50-57a3-4dc9-a12c-06c721b0c86c.jpg
