# Scenario E1: View a publisher-supplied transcript

## Goal
Validate viewing a transcript that the podcast RSS provides (Podcasting 2.0
`<podcast:transcript>`), synced to playback.

## Prerequisites
- App past onboarding. Subscribe to a podcast that ships transcripts in its feed
  (e.g., a show with `<podcast:transcript>`; "This American Life" / "Lex Fridman
  Podcast" episodes often have them). Auto-ingest enabled (Settings → Transcripts).

## Steps
1. Open an episode known to have a publisher transcript and start playback. *Screenshot.*
2. Reveal the transcript (player transcript view / PlayerTranscriptScrollView).
   **Expected:** Transcript segments render with optional speaker labels. *Screenshot.*
3. Let playback advance. **Expected:** The active segment auto-scrolls/highlights as
   the playhead moves (light accent background on the active line). *Screenshot.*
4. Confirm the source is the publisher (not Whisper) — e.g., no "generating"
   indicator; it was present immediately. *Screenshot.*

## Acceptance Criteria
- The transcript renders from the feed without triggering AI transcription.
- The active line tracks playback (auto-scroll + highlight).
- Speaker labels render when present in the transcript format.

## Known Issues / Watch Points
- Publisher transcript formats: JSON (P2.0), WebVTT, SRT — all should parse.
- If the feed doesn't actually supply a transcript, this scenario can't run — choose
  an episode confirmed to have one, or switch to E2 (Whisper) / E4.
- Auto-ingest pre-fetches in the background; the transcript may take a moment to
  appear after subscribing.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, ~4:00 AM**

**Issue: Transcript view UI not accessible; tooling instability prevented full exploration**

Steps attempted:
1. ✓ Step 1 (partial): Opened episode "R.F.K. Jr.'s Newest Mission: Getting Us Off Antidepressants" from The Daily podcast, which was already playing (playhead 7:25)
2. ✗ Step 2 (failed): Could not locate or access PlayerTranscriptScrollView despite multiple attempts to:
   - Scroll episode description view up/down
   - Tap episode title
   - Tap mini-player bar
   - Search UI snapshot for "transcript" keyword (no results found)
3. ✗ Step 3-4: Not reached due to transcript view not being accessible

**Root causes:**
- **No transcript UI visible**: The player UI snapshot showed player controls (play/pause, skip buttons) but no transcript button or tab in accessible locations
- **Possible episode limitation**: The Daily episode being played may not have a publisher-supplied transcript despite description mentioning "Transcripts of each episode will be made available by the next workday"
- **Tooling issue**: xcodebuildmcp daemon experienced timeouts (30s) when tapping certain UI elements (e.g., after opening episode options menu), preventing full exploration of player interface
- **Feature may be unimplemented**: The transcript view may not yet be integrated into the player UI, or it may be hidden behind an undiscovered gesture/button

**Screenshots captured:**
- Initial episode description: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_2992ebf3-ff9b-4ddc-a90b-a53a8bdc817a.jpg
- After reopening app from home: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_f0aab397-54ad-4829-90a3-b98bf90a255c.jpg

**Recommendation:**
- Verify that a known podcast with transcripts (This American Life, Lex Fridman Podcast) is subscribed and an episode is available
- Check if transcript functionality is fully implemented in the player interface
- Investigate if transcript view requires a specific gesture (swipe, long-press) or button that isn't currently visible
- Retry after fixing xcodebuildmcp daemon stability issues
