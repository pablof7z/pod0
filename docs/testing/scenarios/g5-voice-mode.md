# Scenario G5: Voice mode

## Goal
Validate the voice conversation mode: orb states, talk/stop controls, captions, and
switching to text chat.

## Prerequisites
- App past onboarding. LLM provider configured (G1). Microphone permission will be
  requested. Voice is the kernel `podcast.voice` path (canonical).

## Steps
1. Activate voice mode (voice button on the player / agent surface). **Expected:**
   VoiceView presents full-screen with a VoiceOrbView and a state badge "Tap to
   talk". *Screenshot.*
2. Tap the talk button (mic.fill, "Tap to talk"). **Expected:** State → "Listening…"
   (orb expands; label "Listening — tap to send"). *Screenshot.*
3. Speak a short prompt, then tap to send. **Expected:** State → "Thinking…" then
   "Speaking" with the agent's TTS reply; captions show "You" then "Agent" text.
   *Screenshot.*
4. While speaking, tap to interrupt (stop.fill, "Tap to interrupt"). **Expected:**
   Speech stops; mic re-arm is suppressed. *Screenshot.*
5. Tap the **Text** button (keyboard, "Switch to text chat"). **Expected:** Voice
   dismisses and the agent text chat opens. *Screenshot.*
6. Tap close (xmark, "Close voice mode"). **Expected:** Returns to prior screen.

## Acceptance Criteria
- The orb and state badge progress idle → listening → thinking → speaking correctly.
- Talk/stop/interrupt control the conversation; captions show speaker + text.
- "Switch to text chat" hands off to the agent chat; close exits cleanly.
- Voice state is read from the kernel voice projection (reactive).

## Known Issues / Watch Points
- MEMORY (voice_mode_two_paths): the old VoiceView stub stack was non-functional;
  the canonical path is the kernel `podcast.voice` actions + voice snapshot
  projection (VoiceView repointed onto kernel, #552; ElevenLabs sink #551). If voice
  does nothing (no STT, no playback), you may be on stub behavior — capture details.
- Simulator microphone input is limited; STT may not capture real speech. If so,
  validate state transitions/UI and mark audio capture BLOCKED.

## Notes
