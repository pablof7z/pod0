# Scenario K3: Clip composer — verify boundaries snap to transcript utterances, not a fixed window

## Goal
Prove the app's headline differentiator: clip boundaries are **semantically
bounded** (aligned to transcript utterance/sentence edges), NOT a blind fixed
N-second window. This exercises the clip composer's range selector and the kernel's
`resolve_manual_clip_bounds` / `transcript_refine` snapping (see
`apps/nmp-app-podcast/src/clip_boundaries.rs` and `clip_handler.rs`).

## Prerequisites
- A transcribed episode. The transcript surface is reachable only inside the clip
  composer / ask-agent sheets (PlayerTranscriptScrollView is orphaned from the main
  player — see E1). Entry: episode detail → long-press a transcript segment (~600ms
  hold, `HoldToClipGestureModifier`) → `ClipComposerSheet` opens.

## Steps
1. Open the episode detail for a transcribed episode. Trigger the clip composer via
   the long-press / "clip" affordance so `ClipComposerSheet` presents. **Expected:**
   a "Range" label showing `MM:SS → MM:SS`, a `ClipComposerHandlesView` with drag
   handles over transcript segments, a "Caption" field ("Optional headline"), a
   "Subtitle style" picker, and a "Show speaker label" toggle, plus Save/Share.
   *Screenshot.*
2. Read the INITIAL range the composer proposes. Record the start/end timestamps and
   the transcript text spanned. **Expected:** the proposed range begins at the start
   of an utterance and ends at the end of an utterance — i.e. the spanned text reads
   as a complete thought, not a sentence sliced mid-word. *Screenshot.*
3. Drag the **start** handle a little earlier and the **end** handle a little later.
   **Expected:** the displayed `MM:SS` values do NOT move continuously to arbitrary
   offsets; they **snap** to the nearest transcript entry boundary (the handle jumps
   to utterance edges). Note each snapped timestamp. *Screenshot.*
4. Confirm the snap rule qualitatively: the start snaps to the **last** utterance
   whose start ≤ your drag point (backward bias); the end snaps to the **first**
   utterance whose end ≥ your drag point (forward bias). So the clip always fully
   contains complete utterances, never a partial one. *Screenshot.*
5. Enter a caption, Save, and open Clippings. Record the final range. This range is
   what K2/K4 will verify on the relay (`#t=<start>,<end>`). *Screenshot.*

## Acceptance Criteria
- The composer's proposed range spans complete utterances (a coherent thought),
  observably not a fixed offset like "playhead − 30s → playhead".
- Dragging the handles snaps the start/end to transcript utterance boundaries
  (discrete jumps), not continuous arbitrary scrubbing.
- The saved clip's range corresponds to whole-utterance boundaries; quote the
  spanned transcript text in Notes and confirm it begins and ends on sentence/idea
  edges.

## Known Issues / Watch Points
- This is the FLAGSHIP claim. If the handles scrub continuously to any offset and
  the boundaries cut mid-sentence, that is a real FAIL — capture the exact
  start/end and the mid-sentence text in Notes.
- If the episode has NO transcript entries, the kernel uses the raw user range
  as-is (no snap) — that's expected for un-transcribed audio, but then this
  scenario can't validate snapping; pick a transcribed episode.
- The composer measures internally in **milliseconds** (`startMs`/`endMs`) but the
  published `i`-tag rounds to whole seconds — a ≤1s rounding difference between the
  composer display and the relay `#t=` is expected, not a FAIL.

## Notes
