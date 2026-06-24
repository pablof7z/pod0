# Scenario G3: Ask the agent a question about an episode

## Goal
Validate that the agent can answer a question about a specific episode's content
using its transcript/knowledge, including the prefilled-context path from a
transcript segment.

## Prerequisites
- Ollama configured and reachable with `deepseek-v4-flash:cloud` (G1). Live LLM —
  do NOT use the stub here.
- A transcribed episode (E1/E2).

## Steps
1. Open the episode's transcript. Long-press a segment → **Ask the agent about
   this**. **Expected:** Agent chat opens with the segment text + episode context
   prefilled in the composer / as context. *Screenshot.*
2. Ask a concrete question about that part (e.g., "What point are they making
   here?"). Tap send. **Expected:** Typing indicator, then a substantive answer that
   reflects the actual segment content (not a generic non-answer). *Screenshot.*
3. Ask a follow-up referencing the episode broadly (e.g., "Summarize the main
   topics of this episode."). **Expected:** Answer draws on the transcript/knowledge.
   *Screenshot.*
4. Verify the answer is grounded (mentions specifics from the episode). *Screenshot.*

## Acceptance Criteria
- The "Ask the agent about this" path prefills the segment + episode context.
- The agent returns a relevant, grounded answer that references episode content.
- The typing indicator shows during generation; the answer renders as markdown.
- No error bubble (`agent.error`) on a correctly configured provider.

## Known Issues / Watch Points
- If the agent returns generic text unrelated to the episode, the transcript/
  knowledge may not be reaching the prompt — capture the exact Q/A in Notes.
- Replies are non-streaming (one `onPartialContent` call) — the answer appears at
  once, not token-by-token. That's expected.
- A connection error means Ollama isn't reachable (see G1).

## Notes
