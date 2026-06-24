# Scenario B1: Search podcasts by keyword

## Goal
Validate the Search surface returns Shows, Episodes, and Transcripts (kernel
knowledge) result sections for a keyword query.

## Prerequisites
- App past onboarding. Network available.
- For transcript results, having a subscribed/transcribed library helps (optional;
  `--UITestSeed` can preload a library).

## Steps
1. From Home, tap the search affordance (`home-search-button`). **Expected:** Search
   screen with title "Search" and a search bar placeholder "Shows, episodes,
   transcripts". *Screenshot.*
2. Type a keyword (e.g., "fridman"). Wait ~300ms (debounce). **Expected:** Results
   populate under section headers (Shows / Episodes / Transcripts as applicable).
   *Screenshot.*
3. Inspect a **Shows** row. **Expected:** Artwork + title + author. Tap it.
   **Expected:** Navigates to that show's detail (ShowDetailView). *Screenshot.*
4. Go back, inspect an **Episodes** row. **Expected:** Play icon + title + show name
   + snippet; tapping opens the episode detail. *Screenshot.*
5. (If present) inspect a **Transcripts** row (sparkle icon). **Expected:** Title +
   show + snippet + relevance/timestamp; tapping navigates to the episode and may
   seek to the matched position. *Screenshot.*
6. Clear the query. **Expected:** Returns to the empty/prompt state. *Screenshot.*

## Acceptance Criteria
- Typing a keyword returns at least Shows results from the directory.
- Results are sectioned (Shows / Episodes / Transcripts).
- Tapping a show row opens that show; tapping an episode row opens that episode.
- The empty state shows the "Shows, episodes, transcripts" prompt.
- The search debounces (does not fire per keystroke).

## Known Issues / Watch Points
- Transcripts section depends on kernel knowledge-search; it may be empty if nothing
  is indexed — that is expected, not a FAIL.
- Search result rows currently have NO `accessibilityIdentifier` — locate by label
  text via `snapshot_ui`.

## Notes
