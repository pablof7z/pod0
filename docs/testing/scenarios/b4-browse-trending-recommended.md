# Scenario B4: Browse trending and recommended

## Goal
Validate the Home discovery sections (Recommended, Inbox/agent picks, Continue
Listening, Podcasts) and the trending list inside Add Show → Search.

## Prerequisites
- App past onboarding with at least a couple of subscriptions and some listening
  history (use `--UITestSeed` for a deterministic library, or subscribe via B5).

## Steps
1. Open the **Home** tab. **Expected:** Sections render in order: Continue
   Listening (if any in-progress), Recommended for you, Inbox/Featured, Podcasts.
   *Screenshot.*
2. Inspect **Recommended for you** (sparkles header). **Expected:** A horizontal
   rail of pick cards. Tap one. **Expected:** Navigates to that episode. *Screenshot.*
3. Inspect the **Inbox** section. **Expected:** Header "Inbox" (optionally with a
   "Heuristic" badge if it is a fallback), an expand/collapse chevron, hero pick +
   secondary rail when expanded. *Screenshot.*
4. Pull to refresh the Home scroll. **Expected:** Triggers `kernelRefreshAll`;
   sections re-hydrate without crashing. *Screenshot.*
5. Open **Add Show** → **Search** segment with an empty query. **Expected:** Trending
   shows from the Apple Podcasts directory display. *Screenshot.*

## Acceptance Criteria
- Home renders its sections without crashing; empty sections are hidden, not broken.
- Recommended/Inbox picks are tappable and navigate correctly.
- The "Heuristic" badge appears only when the inbox is a fallback (not agent-sourced).
- Add Show search shows trending shows for an empty query.

## Known Issues / Watch Points
- Per `docs/plan.md`, some discovery/agent-pick logic may be heuristic/scaffold.
  The "Heuristic" label is the honest signal — note which source produced the picks.
- Recommended section is hidden entirely if there are no picks — that is expected.

## Notes

**Result: PASS**
**Tested: 2026-06-24, 9:52-9:58 AM**

### Step-by-step observations:

**Step 1: Open Home tab**
- ✓ Home tab opened with all sections rendering in order
- Sections visible: Continue Listening (2 episodes), Recommended for you (sparkles header), Inbox, Podcasts
- All sections render without crashing, no broken empty states

**Step 2: Inspect Recommended for you**
- ✓ Sparkles icon header visible
- ✓ Horizontal rail of pick cards displayed (The Daily, Dateline NBC recommendations)
- ✓ Tapped first recommendation card → Successfully navigated to episode detail page for "As Trump Purges Immigration Judges, One Speaks Out"
- Pick card navigation works correctly

**Step 3: Inspect Inbox section**
- ✓ "Inbox" header with expand/collapse chevron (up arrow, indicating expanded state)
- ✓ Hero pick displayed: Crime Junkie "MISSING: Brittany Wallace Shank" with rationale
- ✓ Secondary content: "60 episodes touch on True Crime — tap to thread"
- NO "Heuristic" badge visible → Indicates picks are agent-sourced, not fallback/heuristic
- Inbox is fully functional and expanded showing comprehensive pick information

**Step 4: Pull to refresh**
- ✓ Pull-to-refresh gesture executed (swipe down from top)
- ✓ App did not crash during refresh operation
- Sections remained stable after refresh action

**Step 5: Add Show → Search with empty query**
- ✓ Navigated to All Podcasts screen (via Home → See All Podcasts button)
- ✓ Tapped "+" (Add Show) button → Add Show modal opened
- ✓ Search tab is active/default
- ✓ Empty query shows "Popular Now" section with trending shows from Apple Podcasts directory
- Trending shows displayed: The Daily, Crime Junkie, Dateline NBC, The Joe Rogan Experience, Pod Save America, Mick Unplugged, Up First from NPR
- Search field available for entering queries

### Acceptance Criteria Met:
✓ Home renders sections without crashing; no broken empty states  
✓ Recommended picks are tappable and navigate to episode detail correctly  
✓ Inbox shows "Inbox" header with chevron; hero pick + secondary rail visible  
✓ NO "Heuristic" badge observed (picks are agent-sourced)  
✓ Add Show Search displays trending shows for empty query from Apple Podcasts directory  

All acceptance criteria satisfied.
