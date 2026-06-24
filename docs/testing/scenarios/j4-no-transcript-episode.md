# Scenario J4: Episode with no transcript

## Goal
Validate the graceful state for an episode that has no transcript (publisher or AI),
including the generate affordance and the absence of transcript-dependent features.

## Prerequisites
- App past onboarding. An episode with NO publisher transcript and AI fallback OFF
  (or no STT provider configured).

## Steps
1. Open an episode with no transcript and play it. *Screenshot.*
2. Reveal the transcript area. **Expected:** An empty/placeholder state with a
   "Generate Transcript" affordance (if actionable) — NOT a crash or blank hang.
   *Screenshot.*
3. Confirm transcript-dependent actions are gracefully unavailable: "Share quote"
   hidden, "Ask the agent about this" segment action unavailable, clip-from-segment
   not offered. *Screenshot.*
4. (If a provider is configured) tap "Generate Transcript" to confirm it transitions
   to transcribing (cross-ref E2). *Screenshot.*

## Acceptance Criteria
- The no-transcript state is a clear placeholder, not a crash/blank.
- A "Generate Transcript" affordance is offered when transcription is actionable.
- Transcript-dependent features (quote share, ask-agent-about-segment, segment clip)
  are hidden/disabled when there is no transcript.

## Known Issues / Watch Points
- "Generate Transcript" is only actionable if a publisher transcript URL exists OR
  an STT provider key is configured — otherwise it should be absent/disabled with a
  hint pointing to Providers.
- AutoSnip may still work without a transcript but the clip lacks contextual naming.

## Notes
