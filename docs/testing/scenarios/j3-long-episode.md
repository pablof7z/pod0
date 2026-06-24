# Scenario J3: Very long episode (>3h)

## Goal
Validate UI and behavior for very long episodes: H:MM:SS time formatting, scrubber
precision, chapter/transcript scale.

## Prerequisites
- App past onboarding. A subscribed show with an episode longer than 3 hours
  (many interview/history podcasts qualify, e.g., long Lex Fridman / Acquired eps).

## Steps
1. Open a >3h episode's detail. **Expected:** Duration meta shows hours (e.g.,
   "3h 24m"). *Screenshot.*
2. Play it. **Expected:** The player time labels use H:MM:SS (current and remaining,
   e.g., "-2:58:11"). *Screenshot.*
3. Drag the scrubber across the full range. **Expected:** Seeking is smooth and the
   position label tracks correctly across hour boundaries. *Screenshot.*
4. Open chapters (if present) — many chapters should render without lag. *Screenshot.*
5. Open the transcript (if present) — a long transcript should scroll/sync without
   freezing. *Screenshot.*

## Acceptance Criteria
- Durations and time labels render in H:MM:SS for >1h content.
- Scrubbing across the full multi-hour range is accurate and smooth.
- Long chapter lists and transcripts render without UI freezes.

## Known Issues / Watch Points
- MEMORY (perf hot paths): large snapshots / many points caused main-thread jank;
  a 3h transcript is a stress test — watch for hangs on open.
- Scrubber precision at multi-hour scale: small drags = large time jumps; confirm
  the 4pt min-distance still gives usable precision.

## Notes
