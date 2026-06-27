# Scenario L4: Transcript-grounded Q&A with live Ollama (`deepseek-v4-flash:cloud`)

## Goal
Prove real inference (not the stub) returns answers GROUNDED in the episode
transcript. Supersedes G3 (BLOCKED on transcript access) by using the
"Ask the agent about this" path from a transcript segment AND by cross-referencing
the agent's answer against E4 transcript search as ground truth.

## Prerequisites
- Ollama reachable (L2) and `deepseek-v4-flash:cloud` selected for the Agent role
  (L1). LIVE LLM — do NOT pass `--UITestAgentStub`.
- A transcribed, indexed episode. E4 confirmed "The Daily" transcripts are indexed
  and searchable (e.g. "As Trump Purges Immigration Judges, One Speaks Out"). Use a
  Daily episode so you have a known-good transcript corpus.

## Steps
1. Establish ground truth first: open the Search tab, search a distinctive term you
   know is in the target episode (E4), and note the exact snippet + timestamp the
   transcript search returns. This is your reference for "grounded". *Screenshot.*
2. Reach a transcript segment in that episode (clip composer / ask-agent surface —
   the transcript is only exposed there, per E1). Long-press a meaningful segment →
   **"Ask the agent about this"** (PlayerTranscriptRow context menu /
   accessibility action "Ask the agent"). **Expected:** the agent chat opens with
   the segment text + episode context prefilled. *Screenshot.*
3. Ask a CONCRETE, answerable-only-from-this-episode question, e.g. for the
   Immigration Judges episode: *"According to this episode, what specific change did
   the judge describe, and why did they decide to speak out?"* Tap send.
   **Expected:** typing indicator, then a substantive answer naming specifics from
   the transcript (the judge's situation, the described change) — NOT a generic "I
   don't have that information" or a non-answer. *Screenshot.*
4. Grounding cross-check: compare the agent's named specifics against the E4 snippet
   from step 1 and the visible transcript. **Expected:** the answer's factual claims
   appear in / are consistent with the transcript. Quote one matching detail in Notes.
5. Negative-control question: ask something the episode does NOT cover, e.g. *"What
   does this episode say about the price of Bitcoin?"* **Expected:** the agent says
   it isn't discussed / declines to fabricate — i.e. it does NOT hallucinate a
   confident wrong answer. *Screenshot.*
6. Broad follow-up: *"Summarize the three main points of this episode."* **Expected:**
   a markdown answer drawing on the transcript, consistent with the episode's actual
   topics. *Screenshot.*

## Acceptance Criteria
- The "Ask the agent about this" path prefills the segment + episode context.
- The concrete question yields a grounded answer whose specifics match the
  transcript (verified against the E4 snippet).
- The negative-control question does NOT produce a hallucinated answer.
- No `agent.error` bubble on a correctly configured live provider; answer renders as
  markdown.

## Known Issues / Watch Points
- If the answer is generic/unrelated to the episode, the transcript/knowledge isn't
  reaching the prompt — capture the EXACT Q and A verbatim in Notes (this is the key
  grounding failure mode).
- Replies are non-streaming (single `onPartialContent`) — the full answer appears at
  once, not token-by-token. Expected, not a FAIL.
- An `agent.error`/connection error means Ollama is unreachable — fix via L2 before
  blaming grounding.
- Do NOT use `--UITestAgentStub` here; the stub's canned reply is not grounded and
  would invalidate the test.

## Notes
