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
**Tested: 2026-06-24, ~8:33 AM**

**Issue: PlayerTranscriptScrollView is not integrated into PlayerView UI**

Steps attempted:
1. ✓ Step 1: Opened episode "137: The Book That Changed Your Life" from This American Life (already playing, playhead 0:00)
2. ✗ Step 2 (failed): Could not find transcript view despite:
   - Opening full player view
   - Searching UI snapshot for "transcript" keyword (no results found)
   - Scrolling within the player sheet content
   - Checking all available tabs/sections

**Root cause analysis:**
- **Feature not integrated into main player**: Code review of PlayerView.swift (lines 1-100) shows a TabView with only two tabs: chapters (tag: false) and show notes (tag: true). No transcript tab exists.
- **PlayerTranscriptScrollView is orphaned**: The source code at App/Sources/Features/Player/PlayerTranscriptScrollView.swift contains a comment stating:
  ```
  // USAGE:
  // Internal-only renderer. No longer referenced from `PlayerView` — the player
  // always shows chapters now. Kept for the clip composer / quote share /
  // ask-agent surfaces that still operate on transcript segments inside their
  // own sheets. Transcripts are an extraction substrate, not user-visible
  // content; do not re-add this as a primary player surface.
  ```
- **This confirms**: Transcript view was deprioritized from the main player UI. The component still exists but is only used internally for clip composition and ask-agent workflows.

**Screenshots captured:**
- Player showing chapters (introduction/main story/conclusion): /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_05c562d6-55e4-4fef-9d65-ba2f6adcb1f6.jpg
- After scroll attempt: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_c5653405-6ef7-443e-a5c4-ff5d5db0307a.jpg
- After scrolling down: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_885b5223-5797-4161-a7b2-96dc9bea4f51.jpg

**Acceptance criteria assessment:**
- ✗ The transcript does NOT render from the feed (not visible in UI)
- ✗ The active line does NOT track playback (no transcript view shown)
- ✗ Speaker labels NOT testable (feature not exposed)

**Conclusion:** This scenario cannot pass until PlayerTranscriptScrollView is re-integrated into PlayerView as a visible tab/section. The infrastructure is in place, but it must be added to the player's TabView layout.
