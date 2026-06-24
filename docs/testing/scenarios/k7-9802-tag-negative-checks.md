# Scenario K7: NIP-84 negative / contract checks (no a-tag, agent clips, empty-transcript)

## Goal
Lock down what the app must NOT do, so a future regression is caught. Verifies three
contract guarantees from the kernel publish path
(`apps/nmp-app-podcast/src/social_publish_handler.rs`):
1. highlights carry an **`i`-tag**, never an **`a`-tag** (F2 wrongly assumed an a-tag);
2. **agent-created** clips do NOT auto-publish via this path;
3. clips with **empty transcript text** do NOT publish (no `context` → skipped).

## Prerequisites
- K1 (`$HEX`). Several clips already created across K2/K3/K6 (manual, transcribed)
  and ideally one agent clip from G4/L5 and one no-transcript clip.

## Steps
1. Pull all of this author's highlights from the session window:
   ```
   nak req -k 9802 -a <HEX> -s <T0> wss://relay.primal.net
   ```
   *Paste the list (ids + alt/context summaries) into Notes.*
2. **No a-tag check:** for every returned event, confirm the `tags` array contains
   NO `["a", …]` entry, and DOES contain an `["i", "podcast:item:guid:…#t=…"]`
   entry. **Expected:** zero a-tags across all events. Any a-tag is a contract
   change — flag it loudly.
3. **Agent-clip exclusion:** in the app, create (or reuse from G4/L5) an
   AGENT-sourced clip (the agent's `podcast.clip.create`). Note its time range and
   text. Re-run the `nak req` from step 1. **Expected:** NO new kind:9802 event
   corresponding to the agent clip appears (agent clips are skipped by
   `publish_clip_highlight_if_user_visible`, which returns early when
   `clip.source == "agent"`). *Document: agent clip range/text, and that it is
   absent from the relay.*
4. **Empty-transcript exclusion:** create a clip on an episode with no transcript
   (or an AutoSnip on un-transcribed audio). **Expected:** it appears in Clippings
   but produces NO kind:9802 event (kernel skips clips with empty
   `transcript_text`). Re-run `nak req` and confirm absence. *Document.*
5. **Signed-in guard (optional):** the publish path requires an active signer
   (`require_signed_in`). If a "no identity" state is reachable, confirm clips do
   not publish while signed out. Otherwise note as not-applicable.

## Acceptance Criteria
- Every published kind:9802 event uses an `i`-tag and NO `a`-tag.
- An agent-sourced clip does NOT yield a kind:9802 event on the relay.
- A clip with empty transcript text does NOT yield a kind:9802 event.
- (If testable) clips do not publish while signed out.

## Known Issues / Watch Points
- Distinguishing "agent clip not published" from "publish just slow" — wait/retry
  the `nak req` a few times and compare the manual-clip control (K2) which SHOULD
  appear in the same window. If the manual clip publishes and the agent clip does
  not within the same window, the exclusion holds.
- An agent clip can be surfaced/undone in the tool batch; an all-undone batch won't
  represent a real clip — make sure the agent clip actually persists in Clippings
  before asserting it should/shouldn't publish.
- If an a-tag DOES appear, capture the full event — this is the most important
  negative finding (it would mean the contract/spec moved).

## Notes
