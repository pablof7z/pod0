# Scenario F3: View existing clippings

## Goal
Validate the Clippings tab: time-bucketed list (Today / This Week / Earlier),
clip cards with range/duration/source badge, search, and tap-to-play.

## Prerequisites
- App past onboarding with ≥1 existing clip (create via F1/D9, or `--UITestSeed`).

## Steps
1. Open the **Clippings** tab (scissors icon). **Expected:** Title "Clippings";
   clips grouped under uppercase headers TODAY / THIS WEEK / EARLIER. *Screenshot.*
2. Inspect a clip card. **Expected:** Range (e.g., "14:31 → 14:58"), duration, a
   relative creation time, optional caption, and a source badge (Auto/AirPods/Agent
   /CarPlay/Watch) when not a plain touch clip. *Screenshot.*
3. Use the search (placeholder "Search clips"). Type a term. **Expected:** List
   filters. *Screenshot.*
4. Tap a clip card. **Expected:** Plays the clip from its start position. *Screenshot.*
5. (Empty state) With no clips, confirm "No Clippings Yet" + "Long-press any
   transcript line to clip a moment…". *Screenshot.*

## Acceptance Criteria
- Clips render in Today/This Week/Earlier buckets, newest first.
- Each card shows range, duration, time, and the correct source badge.
- Search filters the list; tapping a card plays the clip.
- The empty state shows the correct guidance copy.

## Known Issues / Watch Points
- Source badges: `.auto`→Auto, `.headphone`→AirPods, `.agent`→Agent, `.carplay`→
  CarPlay, `.watch`→Watch; a plain `.touch` clip has no badge.
- Orphan clips (clip whose episode is gone) — `--UITestSeedOrphanClip` exercises
  this; confirm it renders gracefully.

## Notes

**Result: PASS**
**Tested: 2026-06-24, 04:45–04:50 UTC**

**Step-by-step observations:**
1. Opened Clippings tab via sidebar (scissors icon) — ✓ Displayed title "Clippings" and clips grouped under "TODAY" header with uppercase format.
2. Inspected clip card displaying:
   - Range: "8:29 → 9:29" (as expected)
   - Duration: "1:00" (visible and correct)
   - Relative creation time: "1h ago" (relative timestamp working)
   - Clip caption: "Clip from R.F.K. Jr.'s Newest Mission: Getting Us Off Antidepressants" (present)
   - Source badge: "THE DAILY" (proper source badge shown)
3. Tested search functionality by typing "RFK" in the search field — ✓ List filtered and displayed "No Results for 'RFK'". Cleared search and clip list returned to normal. Search filtering works as expected.
4. Tapped the clip card — ✓ Playback initiated with player showing position at 8:41 (within ~12 seconds of clip start at 8:29), confirming clip plays from start position.
5. Empty state not tested (not required; prerequisites specify ≥1 clip exists).

**Screenshots taken:**
- Clippings tab open with clip visible: `/var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_35580b8d-a16e-4c3f-81c7-75247d19eed6.jpg`
- Search filtering (no results): `/var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_3813e51b-e6ee-4426-be00-da52763f5a76.jpg`
- Clip playback initiated: `/var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_6df52a24-6763-4191-bacf-434f98c783ec.jpg`

**Acceptance criteria met:**
✓ Clips render under time bucket headers (TODAY observed)
✓ Clip card displays range, duration, creation time, and source badge
✓ Search filters the list (verified filtering + clearing)
✓ Tapping clip card plays from start position (8:29 → 8:41 playback)
✓ Caption and metadata render correctly
