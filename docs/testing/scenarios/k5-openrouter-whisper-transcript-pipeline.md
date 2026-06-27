# Scenario K5: OpenRouter Whisper transcript → clip → relay, full pipeline

## Goal
Drive the complete transcript-to-highlight pipeline on an episode that has NO
publisher transcript: configure OpenRouter Whisper, generate the transcript
on-device-via-OpenRouter, create a clip from a Whisper-derived utterance, and
verify the kind:9802 event on the relay. This is the deepest extension of E2 + F1 +
F2, all of which were BLOCKED individually.

## Prerequisites
- K1 (`$HEX`).
- OpenRouter key configured + validated (I2): key
  key from `~/.tenex/providers.json` → `openrouter.apiKey`.
- Settings → Intelligence → Transcripts → "AI transcription fallback" ON.
- Settings → Intelligence → Models → Speech → **Whisper (OpenRouter)** selected.
  Note: I2 found the Speech provider picker non-responsive under `.pickerStyle(.menu)`;
  PR #617 switched STT/speed pickers to `.pickerStyle(.navigationLink)`. Confirm you
  are on a post-#617 build — the Speech provider should now push a navigation list,
  not a popup menu. If it still uses a popup menu, you are on a stale build; rebuild.
- An episode WITHOUT a publisher transcript, DOWNLOADED (Whisper needs the local
  audio file). E2 noted the seeded fixtures all ship transcripts — so do NOT seed;
  subscribe to a real show and pick an episode with no `<podcast:transcript>` (J4
  documents how to find a no-transcript episode), keep it short.

## Steps
1. Confirm the Speech provider is Whisper (OpenRouter) and the readiness warning in
   Transcripts has cleared. *Screenshot.*
2. Open the no-transcript downloaded episode. **Expected:** a "Generate Transcript"
   affordance (episode detail / transcript fallback). *Screenshot.*
3. Tap **Generate Transcript**. **Expected:** state queued → transcribing
   (TranscribingInProgressView/progress) → ready, without re-opening the player.
   *Screenshot at transcribing and at ready.*
4. Sanity-check the transcript content is real (not empty/garbage): read a few
   utterances; they should be coherent English matching the audio topic. *Screenshot.*
5. Open the clip composer on a Whisper utterance (long-press → `ClipComposerSheet`).
   Confirm the range snaps to the Whisper utterance boundaries (same snap behavior
   as K3, now on AI-generated segments). Set caption `K5-WHISPER-<random>`. Save.
   *Screenshot.*
6. Verify on the relay (host):
   ```
   nak req -k 9802 -a <HEX> -s <T0> -l 10 wss://relay.primal.net
   ```
   **Expected:** a kind:9802 event with your `K5-WHISPER-…` caption in `alt`, a
   `context`/`content` matching the Whisper utterance text, and an `i`-tag
   `podcast:item:guid:<guid>#t=<int>,<int>`. *Paste JSON into Notes.*

## Acceptance Criteria
- With Whisper selected, an episode lacking a publisher transcript transcribes to
  completion with visible progress, producing coherent text.
- A clip created from a Whisper utterance snaps to that utterance's boundaries.
- The resulting kind:9802 event is verifiable on `relay.primal.net` with the
  caption marker, correct `context`/`content`, and a valid `i`-tag.

## Known Issues / Watch Points
- E2 was blocked solely because every fixture episode already had a transcript —
  do NOT use `--UITestSeed`; subscribe to a real show and find a no-transcript
  episode (J4).
- Whisper needs the LOCAL audio — streaming-only won't transcribe; download first.
- If Generate Transcript errors with "Add an OpenRouter API key in Settings →
  Intelligence → Providers", the key did not actually save — redo I2 and confirm
  the "Key validated" card before retrying.
- Long episodes take minutes; choose the shortest no-transcript episode you can.

## Notes
