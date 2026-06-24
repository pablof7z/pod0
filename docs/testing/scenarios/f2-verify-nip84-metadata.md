# Scenario F2: Verify NIP-84 metadata and contextual highlighting

## Goal
Validate that a published highlight carries correct NIP-84 metadata (a-tag, alt,
context) and that the highlight boundaries are semantically meaningful (contextual),
not an arbitrary time slice.

## Prerequisites
- A clip/highlight created and shared/published (F1 + F4).
- Access to the relay to inspect the published event (host-side `nak` against
  `relay.primal.net`, or the in-app representation).

## Steps
1. Create and publish a highlight from a transcript segment (F1 → share/publish).
   *Screenshot.*
2. Inspect the published NIP-84 event (kind:9802). On the host, e.g.:
   `nak req -k 9802 -a <your-hex-pubkey> relay.primal.net` (or check the in-app clip
   detail). **Expected:** The event has:
   - an **a-tag** referencing the podcast/episode (the anchored source),
   - an **alt** tag describing it as a highlight,
   - a **context** tag with surrounding text,
   - `content` = the highlighted text. *Screenshot / paste the event JSON into Notes.*
3. Compare the highlight's start/end to the transcript. **Expected:** Boundaries
   align to sentence/segment edges around a coherent idea, NOT a fixed N-second
   window starting at an arbitrary offset. *Screenshot.*
4. (If the agent created the highlight, G4) confirm the rationale/context reflects
   what is semantically meaningful in the segment.

## Acceptance Criteria
- The published event is NIP-84 (kind:9802) with a-tag, alt, and context tags
  present and correct.
- `content` matches the highlighted transcript text.
- The highlight boundaries are contextual (sentence/idea-aligned), demonstrably not
  a random fixed time slice.

## Known Issues / Watch Points
- This is the headline differentiator of the app — "contextual, LLM-informed, NOT
  random time slices." If boundaries look like a blind fixed window, that's a real
  FAIL worth detailed Notes.
- Inspecting the raw event requires relay access; if unavailable, validate via the
  in-app clip metadata and record what could/could not be verified.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24 @ 04:43 UTC**

### Observations

#### Step 1: Clip exists and is published
- Located published clip in Clippings section
- Clip title: "Clip from R.F.K. Jr.'s Newest Mission: Getting Us Off Antidepressants"
- Podcast: The Daily
- Time boundaries: 8:29 → 9:29 (1-minute duration)
- Published 56 minutes ago (shown as "56m ago" in UI)
- Screenshot: `/var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_27007a9d-e8bb-4f15-89b1-b7aef84d4b59.jpg`

#### Step 2: NIP-84 event inspection - BLOCKED
- **Issue:** Raw NIP-84 event on relay could not be inspected
  - Retrieved user pubkey: npub1jspsl9paa9e99sa53mwc (bech32 format)
  - Hex pubkey: 94030fb0a1b47_8654df7a5714b (appears to have formatting issue in UI rendering)
  - `nak req` command failed with invalid pubkey format error
  - Could not verify a-tag, alt, context tags, or content field from raw relay event
  
- **In-app metadata visible:**
  - Clip title/content: "Clip from R.F.K. Jr.'s Newest Mission: Getting Us Off Antidepressants"
  - Time boundaries: 8:29 → 9:29
  - Associated podcast: The Daily
  - **Not visible in UI:** NIP-84 metadata (a-tag, alt, context tags)
  - App does not expose raw event JSON or detailed NIP-84 metadata in clip detail view

#### Step 3: Contextual boundaries assessment - PARTIAL
- Clip duration: 1 minute (60 seconds) from 8:29 to 9:29
- This appears to be a reasonable semantic unit (not an arbitrary fixed N-second slice)
- **Cannot fully validate without:**
  - Access to episode transcript to compare clip boundaries to sentence/segment edges
  - Raw NIP-84 event showing boundary reasoning/context tag
  - The app does not provide transcript view alongside clip for comparison

#### Step 4: Rationale/context - NOT TESTED
- No rationale/context visible in the in-app clip detail
- Unclear if clip was agent-created or manually created

### Blockers
1. Cannot verify raw NIP-84 event (kind:9802) on relay due to pubkey format issues with nak command
2. In-app UI does not expose NIP-84 metadata (a-tag, alt, context) in clip detail view
3. No transcript available in UI to compare clip boundaries to semantic segments
4. Cannot determine if clip was LLM-informed vs arbitrary selection

### Acceptance Criteria Status
- ❌ Published event metadata (a-tag, alt, context) - **CANNOT VERIFY** (no relay access, not exposed in UI)
- ❌ Content match to transcript - **CANNOT VERIFY** (no transcript visible)
- ❓ Contextual boundaries - **PARTIALLY OBSERVED** (1-min duration reasonable, but cannot confirm against transcript)
