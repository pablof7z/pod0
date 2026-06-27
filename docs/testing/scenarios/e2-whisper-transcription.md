# Scenario E2: Trigger AI (OpenRouter Whisper) transcription

## Goal
Validate generating a transcript via the AI fallback (OpenRouter Whisper) for an
episode the publisher did not provide one for.

## Prerequisites
- OpenRouter key configured (Scenario I2).
- Settings → Transcripts → "AI transcription fallback" ON; Models → Speech → Whisper.
- An episode WITHOUT a publisher transcript, ideally downloaded (transcription needs
  the audio). Use a short episode to keep the run fast.

## Steps
1. Open an episode with no transcript. **Expected:** A "Generate Transcript" button
   (episode detail and/or transcript fallback view) is available. *Screenshot.*
2. Tap **Generate Transcript**. **Expected:** State moves to queued → transcribing
   (a TranscribingInProgressView / progress indicator). *Screenshot.*
3. Wait for completion. **Expected:** State flips to ready; the transcript appears
   without re-opening the player. *Screenshot.*
4. Start playback and confirm the active segment tracks the playhead. *Screenshot.*

## Acceptance Criteria
- With OpenRouter configured, transcription runs and completes, producing a readable
  transcript that syncs to playback.
- Progress is visible during transcription (queued → transcribing → ready).
- If no STT provider key is configured, the UI shows the readiness warning
  ("… needs a key; using Whisper until connected." / "… key not configured —
  connect in Providers.") rather than silently failing.

## Known Issues / Watch Points
- Transcription needs the audio file; streaming-only may not transcribe — download
  first. The on-device Apple STT path also requires a local file.
- Whisper errors prompt "Add an OpenRouter API key in Settings → Intelligence →
  Providers" — confirm the key is actually saved (I2) if you see this.
- Long episodes take minutes; pick a short one.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, ~4:20am**

### Observations:

**Previous Session (4:08am):**
- UI automation daemon was timing out on snapshot-ui calls
- Unable to retrieve element references for episode selection

**Current Session (4:20am):**
- UI automation daemon is NOW WORKING: snapshot-ui returns element refs successfully
- OpenRouter API key is still configured in Settings
- Successfully navigated to app and opened episode player
- Episode 137 "The Book That Changed Your Life" from "This American Life" is loaded

**Blocker: No Episode Without Publisher Transcript Found**
- Opened episode 137 in detail view
- Episode shows 3 chapters/segments: "Introduction" (0:00), "Main Story" (1:00), "Conclusion" (3:00)
- Chapter/segment data indicates this episode HAS a publisher-provided transcript
- The prerequisite requires "An episode WITHOUT a publisher transcript"
- Did not see "Generate Transcript" button in the UI (only chapter navigation visible)
- Attempted to scroll episode list to find alternative episodes, but available episodes in the fixture data also have chapters

### What Was NOT Tested:
- Step 1: Finding and opening an episode WITHOUT publisher transcript
- Step 2: Tapping "Generate Transcript" button (not visible because episodes have transcripts)
- Step 3: Waiting for transcription to complete
- Step 4: Playback sync with AI-generated transcript segments

### Code Investigation:
- OpenRouterWhisperClient.swift is fully implemented with error handling
- FFI bridge to Rust transcription API is wired up (nmp_app_podcast_openrouter_whisper_transcribe)
- Transcript domain model supports both publisher and whisper sources
- Implementation appears ready but UI fixture data lacks episodes without transcripts

### Recommendation:
To complete this scenario, need to find or create an episode in the app's library that has:
1. No publisher-provided transcript/chapters
2. Downloaded audio (required for transcription)
3. Short duration (to keep test fast)

Workaround: Either:
- Add a test episode without transcript to fixture data
- Manually search and subscribe to a podcast episode known to lack publisher transcript
- Use a different test episode source
