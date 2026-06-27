# Scenario L7: Cross-reference transcript search (E4) with agent answers

## Goal
Use the kernel transcript/knowledge search (E4, PASS) as an independent oracle for
the agent's grounding. Whatever the agent claims about an episode must be findable
in the indexed transcript via Search; whatever the agent says is absent must NOT be
findable. This catches subtle hallucination that a single-surface test misses.

## Prerequisites
- E4 working (transcript search returns snippets + timestamps — confirmed PASS).
- Live Ollama (L2) + `deepseek-v4-flash:cloud` (L1). Not the stub.
- A transcribed, indexed episode (Daily).

## Steps
1. Ask the agent (in that episode's context) a factual question; get an answer that
   names a specific claim/quote/entity. Record the exact specific (e.g. a name, a
   number, a phrase). *Screenshot.*
2. Open the Search tab and search that specific phrase/entity. **Expected:** a
   Transcripts section result for the SAME episode, with a snippet containing the
   term and a timestamp. The agent's claim is corroborated by the index. *Screenshot.*
3. Tap the transcript result. **Expected:** it navigates to the episode at the
   matched position (E4). Confirm the surrounding text supports the agent's claim.
   *Screenshot.*
4. Reverse direction: pick a distinctive phrase from a transcript search snippet and
   ask the agent about it specifically. **Expected:** the agent's answer is
   consistent with that snippet (no contradiction). *Screenshot.*
5. Hallucination probe: ask the agent for a "quote" about a topic the episode does
   NOT cover. Take whatever phrase it offers and search it in the Transcripts index.
   **Expected:** EITHER the agent refused to fabricate (best), OR if it produced a
   phrase, that phrase is NOT found in the index → flag as a hallucination. *Screenshot.*

## Acceptance Criteria
- Specific claims in the agent's grounded answers are corroborated by a transcript
  search hit in the same episode.
- A phrase taken from a transcript snippet yields a consistent agent answer.
- The hallucination probe either elicits a refusal or, if a fabricated phrase is
  produced, the absence from the index is documented as a finding.

## Known Issues / Watch Points
- Knowledge search is kernel-side `top_k_search` (linear scan) and only covers
  INDEXED transcripts — a freshly transcribed episode may need a moment to index;
  if Search returns nothing for a known phrase, wait and retry before concluding the
  agent hallucinated.
- Snippet vs answer wording will differ (paraphrase) — judge on semantic match of
  the specific (name/number/claim), not exact string equality.

## Notes
