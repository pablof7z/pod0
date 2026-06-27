# Scenario M1: Voice mode via the canonical kernel path (VoiceView, podcast.voice)

## Goal
Exercise the CANONICAL voice conversation — the kernel `podcast.voice` path that
drives `VoiceView` (orb states from the voice snapshot projection) — NOT the
episode-scoped "Voice note" recording sheet that G5 accidentally opened. G5 was
BLOCKED because it tapped the player's mic button, which opens
`VoiceNoteRecordingSheet` (a half-sheet, immediate "Listening…", local STT), not the
full-screen `VoiceView`. This scenario reaches the right surface and verifies the
reactive state machine.

## Prerequisites
- App past onboarding; LLM provider configured (L2). Microphone permission will be
  requested.
- The canonical entry point: `VoiceView` is presented as a `fullScreenCover` from
  RootView in response to the `.voiceModeRequested` notification, posted by
  `StartVoiceModeIntent` (the "Start Voice Mode" App Intent / Siri shortcut). There
  is NO plain in-app button wired to it as of the G5 run — so trigger it via the App
  Intent / Siri shortcut "Start Voice Mode" (or whatever invokes
  `StartVoiceModeIntent`). Do NOT use the player mic button (that's the Voice note
  sheet).

## Steps
1. Trigger `StartVoiceModeIntent` (Siri / Shortcuts "Start Voice Mode", or the
   intent invocation seam). **Expected:** a FULL-SCREEN `VoiceView` (dark scheme)
   with a `VoiceOrbView` and a state badge reading **"Tap to talk"** (idle) — NOT a
   half-sheet titled "Voice note", and NOT an immediate "Listening…". *Screenshot.*
   - If you instead get a half-sheet "Voice note" with immediate recording, you hit
     the WRONG surface (the player mic button / `VoiceNoteRecordingSheet`). Back out
     and use the App Intent. Distinguish the two clearly in Notes.
2. Confirm the three action-row buttons by accessibility label: **"Close voice
   mode"**, the central talk button (label varies by state — idle: **"Tap to talk"**),
   and **"Switch to text chat"**. *Screenshot.*
3. Tap the talk button. **Expected:** state badge → **"Listening…"**; the talk
   button's label becomes **"Listening — tap to send"**; the orb expands
   (VoiceOrbState `.listening`). This state must come from the kernel voice snapshot
   projection (reactive), not local-only UI. *Screenshot.*
4. Speak a short prompt (sim mic is limited — see Watch Points), then tap to send.
   **Expected:** state → **"Thinking…"** (orb `.thinking`), then **"Speaking"** with
   the agent's TTS reply; the caption rail shows a "You" row then an "Agent" row.
   *Screenshot at thinking and speaking.*
5. While the agent is speaking, tap to interrupt (label **"Tap to interrupt"**,
   stop.fill). **Expected:** speech stops; mic re-arm is suppressed (barge-in).
   *Screenshot.*
6. Tap **"Switch to text chat"**. **Expected:** VoiceView dismisses and the agent
   text chat (L3) opens. *Screenshot.*
7. Re-open VoiceView and tap **"Close voice mode"**. **Expected:** returns to the
   prior screen cleanly. *Screenshot.*

## Acceptance Criteria
- The canonical full-screen `VoiceView` is reached (idle "Tap to talk"), not the
  episode "Voice note" half-sheet.
- The orb + state badge progress idle → listening → thinking → speaking, driven by
  the kernel voice snapshot projection.
- Talk/send/interrupt control the conversation; the caption rail shows speaker +
  text.
- "Switch to text chat" hands off to the agent chat; "Close voice mode" exits
  cleanly.

## Known Issues / Watch Points
- MEMORY (voice_mode_two_paths): the OLD VoiceView stub stack was non-functional;
  the canonical path is kernel `podcast.voice` + voice snapshot projection
  (repointed #552; ElevenLabs sink #551). If the orb never leaves idle, there's no
  TTS playback, or states don't advance, you may be on stub behavior or the kernel
  voice projection isn't bumping — capture exactly which states did/didn't fire.
- Simulator microphone input is limited; real STT may not capture speech. If so,
  validate the STATE TRANSITIONS and UI (badge/orb/captions/buttons) and mark only
  the audio-capture sub-step BLOCKED — the kernel-reactive state machine is the
  primary thing under test.
- If there is genuinely no way to invoke `StartVoiceModeIntent` from the
  sim/Shortcuts, record that the canonical VoiceView has no reachable entry point in
  this build (a real gap) and reference G5.

## Notes
