# Scenario K6: AutoSnip (D9) → NIP-84 publish → relay verification

## Goal
Verify the AutoSnip path (D9) produces a user-visible, transcript-bearing clip that
auto-publishes as a kind:9802 highlight, and that its boundaries are
utterance-refined (not a raw 30s audio slice). Combines D9 with K2/K4 relay
verification. D9 itself was BLOCKED on sidebar navigation to Clippings — this
scenario also re-tests that path.

## Prerequisites
- K1 (`$HEX`).
- A transcribed, DOWNLOADED episode playing (AutoSnip refines against the
  transcript; without one it falls back to a raw window).
- Signed in (publish requires an active signer).

## Steps
1. Record `$T0` on the host (`date -u +%s`). Start playback; let it run past ~0:45
   so AutoSnip's ~30–60s lookback has material. *Screenshot.*
2. Tap **AutoSnip** (bookmark.fill, "Snip last 30 seconds"). **Expected:** an
   AutoSnipBanner ("Clipped: <title>"). *Screenshot.*
3. Navigate to **Clippings** (via the sidebar/avatar entry — D9 found this
   unresponsive while the full player sheet was up; first dismiss the full player
   sheet, then open the sidebar). **Expected:** a new clip at the top, "just now".
   *Screenshot.* If the sidebar is still unreachable, capture it and mark the
   navigation BLOCKED (separately from the publish check).
4. Confirm the clip is NOT agent-sourced (no Agent/sparkles badge → it's a touch
   snip) and HAS transcript text. Only such clips auto-publish. Record its range.
   *Screenshot.*
5. Verify the relay event:
   ```
   nak req -k 9802 -a <HEX> -s <T0> -l 10 wss://relay.primal.net
   ```
   **Expected:** a kind:9802 event whose `i`-tag `#t=<a>,<b>` matches the AutoSnip
   range, `context`/`content` = the snipped transcript text, and `alt` = the clip
   title (AutoSnip titles are often a chapter title or AI caption). *Paste JSON.*
6. Boundary judgment: the AutoSnip targets ~30–60s but then snaps to utterance
   edges (`clip_boundaries.rs::transcript_refine`). Read the `content`: it should
   begin/end on utterance boundaries even though the duration is ~30s. A ~30s
   duration is fine; mid-word cuts at the edges are NOT. *Document in Notes.*

## Acceptance Criteria
- AutoSnip creates a clip ending at the current moment, ~30–60s, that appears in
  Clippings.
- The clip auto-publishes a kind:9802 event verifiable on `relay.primal.net`
  (created after `$T0`) with a matching `i`-tag range and transcript `content`.
- The AutoSnip boundaries land on transcript utterance edges (the ~30s target is
  refined to complete utterances), not raw audio cuts.

## Known Issues / Watch Points
- If no LLM key is set, the clip may lack an AI caption (NoLLMKeyHintBanner) — the
  `alt` tag may then be empty/omitted, which is allowed. Not a publish FAIL.
- D9's sidebar-to-Clippings navigation was blocked while the full player sheet was
  presented; dismiss the sheet first. Track that UI issue separately from the
  relay-publish result — the publish can succeed even if the Clippings UI is hard
  to reach (verify via `nak`).
- AutoSnip without a transcript yields no `context` text → it will NOT auto-publish
  (the kernel skips empty-transcript clips). Use a transcribed episode.

## Notes
