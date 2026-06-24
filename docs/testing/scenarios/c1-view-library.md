# Scenario C1: View subscribed podcasts library

## Goal
Validate the Library tab renders subscribed podcasts (grid/list) with artwork,
titles, authors, and unplayed counts.

## Prerequisites
- App past onboarding with ≥2 subscriptions (use `--UITestSeed` or subscribe via B5).

## Steps
1. Open the **Library** tab (tray.fill icon). **Expected:** Subscribed shows render
   as grid cells / rows (`library-podcast-list` / `library-podcast-row`). *Screenshot.*
2. Inspect a cell. **Expected:** Artwork tile, title (≤2 lines), author (1 line),
   optional category badge, and an unplayed-count indicator when >0. *Screenshot.*
3. Apply a library filter / category scope if available. **Expected:** List narrows;
   an empty filter shows a "No shows in <Category> yet" state with "Clear filters".
   *Screenshot.*
4. Tap a podcast cell. **Expected:** Navigates to ShowDetailView. *Screenshot.*
5. Long-press a Home Podcasts row. **Expected:** Context menu offers unsubscribe.
   *Screenshot.*

## Acceptance Criteria
- All subscribed shows are listed with correct artwork/title/author.
- Unplayed counts render and look plausible.
- Filtering/scoping narrows the list and offers a clear-filters escape.
- Tapping a show opens its detail.

## Known Issues / Watch Points
- Large libraries previously caused main-thread jank on snapshot decode (MEMORY:
  perf hot path). Watch for a visible hang while the grid loads.
- Empty library should show the first-run empty state ("Your shows live here.").

## Notes

**Result: PARTIAL**
**Tested: 2026-06-24, ~10:32**

Navigation & View Discovery:
- Found subscribed podcasts accessible via: Home page → "See all podcasts" OR Sidebar → Podcasts
- View is titled "All Podcasts" and displays subscribed shows in a vertical list (rows)
- Current view does NOT match the expected element IDs (`library-podcast-list` / `library-podcast-row`)

Podcast Cell Contents (Step 2):
- ✓ Artwork tile present (colorful podcast logos)
- ✓ Title visible (≤2 lines, e.g., "Crime Junkie", "Hard Fork")
- ✓ Author/Provider visible (1 line, e.g., "Audiochuck", "NBC News", "The New York Times")
- ✗ Unplayed-count indicator: NOT VISIBLE in this view (shows "Following" badge instead)
- ✗ Category badge: NOT VISIBLE

Acceptance Criteria Status:
- ✓ All subscribed shows listed with correct artwork/title/author (5 podcasts visible: Crime Junkie, Dateline NBC, Hard Fork, NPR Topics: News, The Daily)
- ✗ Unplayed counts: Not rendered in the "All Podcasts" view
- ✗ Filtering/scoping: No filter tabs or category filters visible in "All Podcasts" view
- ✗ Tapping a show: Tapped Crime Junkie, card entered selected state but did not navigate to ShowDetailView

Additional Findings:
- Sidebar navigation found: Home, Library (episode library), Podcasts, Bookmarks, Clippings
- "Library" tab shows episodes, not podcasts
- "Podcasts" tab shows subscribed podcasts in a list with episode counts and "Following" badges
- Long-press context menu for unsubscribe (Step 5) not tested due to command syntax requirements

**Assessment:** The "All Podcasts" view is functional for discovering subscribed shows, but the current implementation does not match the scenario's expectations for unplayed counts, filtering/category scoping, or the specific element IDs (`library-podcast-row`). The scenario may be testing a different view or an older version of the app UI.
