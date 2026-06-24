# Scenario C5: Episode detail view

## Goal
Validate the EpisodeDetailView layout and actions: hero, AI categories, play/queue/
download action row, summary, chapters, show notes, comments, diagnostics.

## Prerequisites
- App past onboarding with a subscribed show; ideally an episode with chapters and
  AI categories (use `--UITestSeed` which seeds chapters for ep1).

## Steps
1. Open an episode's detail. **Expected:** Hero (artwork 110×110, title, show name,
   "MMM d, yyyy · Xh Ym" meta). *Screenshot.*
2. If present, inspect the **AI Categories** pill row. **Expected:** Horizontal
   category chips. *Screenshot.*
3. Inspect the **action row**: Play/Resume/Play again, Queue/Queued, Download pill.
   **Expected:** Labels reflect current state. *Screenshot.*
4. Read the **Summary** (first sentence, italic/quoted) and **Show Notes**
   (HTML-rendered). *Screenshot.*
5. Inspect **Chapters** (timestamp + title + summary; active chapter shows a
   waveform). Tap a chapter. **Expected:** Seeks and starts playback. *Screenshot.*
6. Scroll to **Comments** (only renders for episodes with a Podcasting 2.0 GUID).
   *Screenshot.*
7. Toolbar (…) → **Diagnostics**. **Expected:** EpisodeAuditLogView opens. *Screenshot.*

## Acceptance Criteria
- Hero, summary, show notes, and chapters render correctly.
- Action-row labels match the episode state (Play vs Resume vs Play again; Queue vs
  Queued; Download vs Downloading vs Downloaded vs Retry).
- Tapping a chapter seeks to its start and plays.
- Diagnostics opens an audit log.

## Known Issues / Watch Points
- Comments section is conditional on a P2.0 GUID — absence is expected for some feeds.
- Show notes are HTML-rendered; watch for unescaped markup or layout breakage.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24 10:52 UTC**

**Issue:** Prerequisites require app to be started with `--UITestSeed` flag to seed test data (chapters for ep1), but app was launched without this flag.

**Observations:**
- App opened to Home screen with "Your shows live here" message
- No subscriptions present in the library
- Attempted to add a show manually via "Add Show" button, but UI navigation led to Identity page instead
- Cannot proceed to Episode Detail view testing without a subscribed show

**Blocking factor:** 
- App needs to be restarted with `--UITestSeed` flag to populate test data (seeded podcast with chapters)
- xcodebuildmcp CLI may not support custom launch arguments
- Manual show subscription would require Apple Podcasts feed lookup + subscription flow, which is time-consuming for a quick test scenario

**Recommendation:**
Restart the simulator app with the `--UITestSeed` launch argument before running this scenario, or pre-populate the app database with test show data.
