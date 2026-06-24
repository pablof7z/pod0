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
**Tested: 2026-06-24, ~4:08am**

### Observations:

**Step 1: Configure OpenRouter API Key**
- Successfully navigated to Settings → Intelligence → Providers → OpenRouter
- Entered OpenRouter API key
- Tapped Save button
- Key was successfully stored: Settings now shows "Providers: 1 connected" and OpenRouter shows "Manual" connection status
- Confirmed key saved in Keychain

**Blocker: UI Automation Daemon Timeout**
- After restarting the app to ensure clean state, the UI automation daemon began consistently timing out when attempting to capture runtime UI snapshots
- Unable to get element references (elementRefs) needed to tap on episodes
- This prevented testing Steps 1-4 of the transcription workflow (opening episode, tapping "Generate Transcript", waiting for completion, testing playback sync)

### What Was NOT Tested:
- Step 1: Opening episode without publisher transcript and finding "Generate Transcript" button
- Step 2: Tapping "Generate Transcript" and observing state transition (queued → transcribing)
- Step 3: Waiting for transcription to complete and transcript to appear
- Step 4: Playback sync with transcript segments

### Technical Details:
- Simulator: 9956D3C2-466B-4005-A5FF-1B018B8DE734 (podcast-iter, iOS 26.5)
- App bundle: io.f7z.podcast
- Xcode daemon service hung/timed out on `ui-automation snapshot-ui` calls after app restart
- Subsequent screenshot commands continued to work, but snapshot-ui returned consistent 30s timeouts

### Recommendation:
The OpenRouter key configuration prerequisite is confirmed working. To complete this scenario:
1. Restart the xcodebuildmcp daemon service (or restart the host's test runner) to recover from the timeout
2. Rerun the scenario with focus on Steps 1-4
3. Alternatively, verify transcription feature locally on a development machine with a working UI automation daemon
