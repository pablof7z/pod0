# Scenario K4: Cross-reference the relay event's `content`/`context` against the transcript

## Goal
Prove the published highlight's text is the actual transcript excerpt for the
clip's time range — i.e. `content` (and the `context` tag) is the real spoken text
between `#t=<start>,<end>`, and it is a complete thought. This closes the loop
between K2 (the raw event) and K3 (the in-app boundaries) using the transcript as
ground truth.

## Prerequisites
- K1 (`$HEX`), K2 (a verified kind:9802 event for a known clip), K3 (the clip's
  in-app range and the transcript text it spans, recorded).

## Steps
1. Re-fetch the specific event from K2 and isolate its fields:
   ```
   nak req -k 9802 -a <HEX> -s <T0> -l 10 wss://relay.primal.net
   ```
   From the matching event, extract: `content`, the `context` tag value, and the
   `i`-tag `#t=<start>,<end>`. *Paste into Notes.*
2. Confirm `content` == the `context` tag value (the kernel sets them to the same
   highlighted text). A mismatch is a FAIL worth flagging.
3. Open the same clip in the app and view its transcript excerpt (the clip preview
   in `ClipComposerSheet` / Clippings shows the spanned text). Compare word-for-word
   against the event `content`. **Expected:** they match (allowing for whitespace
   normalization). *Screenshot.*
4. Semantic-boundary check (the headline differentiator): read the `content` text
   on its own. **Expected:** it starts at the beginning of a sentence/idea and ends
   at the end of one — a self-contained excerpt, NOT "…ddle of a word" or a clause
   cut off mid-thought. Write a one-line judgment in Notes: *complete thought?
   yes/no*, quoting the first and last ~6 words.
5. Window-vs-context check: compute the duration `end − start` from the `i`-tag.
   **Expected:** the duration follows the transcript span (it varies with the
   utterances captured), and is NOT pinned to a magic constant like exactly 30s for
   every clip. If multiple clips were made (K2 + K3 + D9), compare their durations —
   they should differ, reflecting different idea spans, except AutoSnip (D9/M-AutoSnip)
   which intentionally targets ~30s. Note which path produced which duration.

## Acceptance Criteria
- The relay event's `content` equals its `context` tag value and equals the in-app
  clip transcript text.
- The text reads as a complete thought bounded at sentence/idea edges (documented
  with the first/last words quoted).
- The `#t=` duration reflects the transcript span (variable across manual clips),
  demonstrably not a single hard-coded window for all clips.

## Known Issues / Watch Points
- AutoSnip (D9) intentionally targets a ~30–60s window then refines to utterance
  edges, so a ~30s AutoSnip clip is NOT evidence of "blind fixed window" — judge it
  by whether its edges land on utterance boundaries, not by the round duration.
- Whitespace/newline differences between the relay `content` and the UI rendering
  are cosmetic — normalize before comparing.
- If `content` is clearly a raw audio time-slice with words cut at both ends, that
  contradicts the product claim — capture it verbatim; this is the single most
  important failure mode to document.

## Notes
