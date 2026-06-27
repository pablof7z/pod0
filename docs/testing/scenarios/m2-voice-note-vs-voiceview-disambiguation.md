# Scenario M2: Disambiguate the two voice paths (Voice note sheet vs kernel VoiceView)

## Goal
Document, deterministically, the difference between the app's two voice surfaces so
future runs stop conflating them (the root cause of G5's BLOCKED result). One is the
episode-scoped **Voice note** half-sheet (player mic button → `VoiceNoteRecordingSheet`,
local STT, dispatches a note into the agent chat); the other is the canonical
kernel **VoiceView** (M1). This scenario walks BOTH and records their distinct
signatures.

## Prerequisites
- App past onboarding, an episode loaded in the player. Mic permission grantable.
- LLM provider configured (L2) for the Voice note → agent dispatch to do anything
  useful.

## Steps
1. **Voice note path:** open the full player; tap the player's mic button
   (PlayerControlsView, accessibilityLabel **"Voice note"**). **Expected:** a
   HALF-SHEET (`.fraction(0.45)`/`.medium`) titled **"Voice note"** with a mic orb
   and **Cancel**/**Send** buttons, entering recording ("Listening…") immediately.
   It captures the current episode + playback position + chapter context. *Screenshot.*
2. Speak (or attempt to) and tap **Send**. **Expected:** the voice note is dispatched
   as a `VoiceNoteAgentContext` into the agent chat (a new agent message referencing
   the episode/position). Open the agent chat to confirm the note arrived. *Screenshot.*
   Tap **Cancel** on a fresh recording to confirm it dismisses without dispatching.
3. **Kernel VoiceView path:** trigger `StartVoiceModeIntent` (M1 step 1).
   **Expected:** the FULL-SCREEN VoiceView with idle "Tap to talk" — visibly
   different from the half-sheet. *Screenshot.*
4. Build the disambiguation table in Notes: for each path record — entry point,
   presentation (half-sheet vs full-screen), initial state (immediate "Listening…"
   vs idle "Tap to talk"), STT source (local VoiceNoteRealtimeSTT vs kernel
   podcast.voice), and outcome (note dispatched into chat vs live STT→LLM→TTS loop).

## Acceptance Criteria
- The player mic button opens the half-sheet "Voice note" (local, episode-scoped),
  which dispatches a note into the agent chat on Send and dismisses on Cancel.
- `StartVoiceModeIntent` opens the full-screen kernel VoiceView (idle "Tap to talk").
- The two surfaces are documented with their distinguishing signatures so they are
  no longer conflated.

## Known Issues / Watch Points
- G5 mistook the Voice note half-sheet for VoiceView and reported "no idle state" —
  that's correct for the Voice note sheet (it records immediately) and is NOT a bug;
  the bug-shaped finding is only that VoiceView lacks an obvious in-app entry point.
- Simulator mic limits real capture for BOTH paths; validate the surface
  identification and dispatch/state behavior even if audio is empty.
- If Send on the Voice note sheet does not produce an agent message, capture it —
  that's a real dispatch failure (distinct from the VoiceView path).

## Notes
