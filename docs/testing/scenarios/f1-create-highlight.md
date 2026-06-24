# Scenario F1: Create a NIP-84 highlight from a transcript segment

## Goal
Validate creating a clipping/highlight from a transcript segment, including the
range selector and caption, and saving it.

## Prerequisites
- App past onboarding with a transcribed, playing episode (E1/E2).

## Steps
1. Open the player transcript. Long-press a meaningful segment. **Expected:** A
   context menu (includes "Ask the agent about this"; clip/highlight affordance).
   *Screenshot.*
2. Invoke the clip composer (from the transcript segment or the AutoSnip flow).
   **Expected:** Composer with a range selector (drag handles, sentence-snap), an
   optional caption/headline field, a "Show speaker label" toggle, and Save/Share.
   *Screenshot.*
3. Adjust the range with the drag handles. **Expected:** Start/end timestamps update
   (HH:MM:SS → HH:MM:SS). *Screenshot.*
4. Enter a caption. Tap **Save**. **Expected:** The clip is created and saved.
   *Screenshot.*
5. Open the **Clippings** tab. **Expected:** The new clip appears at the top with its
   range, caption, and "just now" timestamp. *Screenshot.*

## Acceptance Criteria
- A highlight can be created from a transcript segment with an adjustable range.
- The caption is saved and shown in the Clippings list.
- The clip's start/end map to the selected transcript boundaries (not a raw window).
- The saved clip persists across navigation.

## Known Issues / Watch Points
- The point of NIP-84 here is CONTEXTUAL highlighting — the default range should
  snap to sentence/segment boundaries, not an arbitrary 30s window. Verify F2.
- If there is no transcript, the composer can't anchor a segment — use a transcribed
  episode.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, ~4:30 AM**

**Issue: Prerequisite feature (transcript UI) not yet implemented**

Steps attempted:
1. ✓ Launched app and navigated to episode player (The Daily: "As Trump Purges Immigration Judges")
2. ✗ Could not locate transcript view in player UI
3. ✗ Transcript segment long-press cannot be executed without accessible transcript view

**Root cause:**
The transcript UI (PlayerTranscriptScrollView) is defined in the codebase but not yet integrated into the player interface. Code comments explicitly state:
- "Transcripts are an internal extraction layer" (EpisodeDetailView.swift, lines 6-14)
- "The transcript is never rendered as a primary surface" (PlayerNoChaptersPlaceholder.swift, lines 6-9)
- The transcript serves only as an internal substrate for chapters, RAG, and agent tools

The current player UI only provides two tabs:
1. Chapters panel (or "No chapters" placeholder when unavailable)
2. Show notes

**Blockers:**
- No button, tab, or navigation element to access transcript view in the player
- No way to long-press transcript segments without the transcript UI being visible
- Feature appears to be scaffolded architecturally but not yet integrated into the user-facing player UI

**Recommendation:**
- This scenario cannot proceed until the transcript UI is integrated into the player as a discoverable, interactive surface
- Prerequisite: Implement/wire PlayerTranscriptScrollView into the player's tab view or navigation
- Related: Scenario E1 (View Publisher Transcript) has the same blocker and status
