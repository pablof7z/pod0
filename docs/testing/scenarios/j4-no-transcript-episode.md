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

**Result: BLOCKED**
**Tested: 2026-06-24, ~1:40 AM**

The episode "137: The Book That Changed Your Life" from "This American Life" was opened and examined. The episode view displays:
- Summary section with episode description
- Chapters section (Introduction, Main Story, Conclusion)
- Show notes
- Comments section

**Key Findings:**
- No explicit "Transcript" section is visible in the episode detail view
- No "Generate Transcript" affordance or placeholder is displayed anywhere
- No transcript-dependent action buttons (Share quote, Ask agent) are visible
- The UI snapshot shows 39 targets but none related to transcript functionality

**Unexpected Behavior:**
The scenario expects to see either:
1. A clear placeholder state for "no transcript" with a "Generate Transcript" button, OR
2. At least a hint about configuring a provider

Instead, the transcript feature appears to be completely absent from the UI - not shown as unavailable or with a placeholder, but simply not present in the episode view.

**Status:**
- Acceptance criterion #1 (clear placeholder) — NOT MET (no placeholder visible)
- Acceptance criterion #2 (Generate affordance) — NOT MET (no button visible)
- Acceptance criterion #3 (transcript features hidden) — PARTIALLY MET (they're hidden because transcript section doesn't exist)

**Blocker Reason:**
Cannot fully test the scenario because the expected transcript UI section/affordance does not exist in the current build. The feature may be:
1. Not yet implemented for no-transcript state
2. Conditionally hidden based on feature flags
3. Only visible after a specific action or state change not yet performed

**Recommendation:**
Verify that transcript UI is expected to be visible in this episode view, or clarify the conditions under which the "Transcript" section should appear.
