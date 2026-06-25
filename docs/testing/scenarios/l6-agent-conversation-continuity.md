# Scenario L6: Agent conversation continuity and multi-turn grounding

## Goal
Validate a multi-turn conversation with live Ollama holds context across turns and
across the history/new-conversation boundary: follow-ups resolve pronouns to the
earlier episode, a NEW conversation starts clean, and resuming an old conversation
from history restores its context.

## Prerequisites
- Live Ollama (L2) + `deepseek-v4-flash:cloud` for the Agent role (L1). Not the stub.
- A transcribed, indexed episode (Daily, per L4).

## Steps
1. Open the agent chat from a transcript segment (L4 step 2) so an episode is in
   context. Ask a first question about the episode. Get a grounded answer (L4).
   *Screenshot.*
2. Ask a context-dependent FOLLOW-UP that only resolves if the prior turn is
   retained, e.g. *"Who said that?"* or *"What happened right after that moment?"*
   **Expected:** the answer references the SAME subject/episode from turn 1, not a
   reset. *Screenshot.*
3. Tap "Conversation history". **Expected:** this conversation is listed and
   checkmarked. Note its title. *Screenshot.*
4. Tap "New conversation". Ask a question with a dangling pronoun, e.g. *"What was
   their main argument?"* with no prior context. **Expected:** the agent does NOT
   carry over the previous conversation's subject — it asks for clarification or
   answers generically, proving the new conversation is isolated. *Screenshot.*
5. Open history again and tap the FIRST conversation to resume it. Ask another
   follow-up referencing the original episode. **Expected:** context is restored;
   the answer is grounded in the original episode again. *Screenshot.*

## Acceptance Criteria
- A follow-up within one conversation resolves to the earlier turn's subject
  (context retained across turns).
- A new conversation is isolated — it does not inherit the prior conversation's
  episode context.
- Resuming a past conversation from history restores its context for subsequent
  turns.

## Known Issues / Watch Points
- If follow-ups reset every turn, conversation memory isn't reaching the prompt —
  capture the turn-by-turn Q/A.
- Replies are non-streaming (appear at once). Expected.
- Keep prompts short; deepseek-v4-flash should respond quickly, but a slow/locally
  overloaded Ollama can stall — distinguish a stall (no reply) from a context-loss
  (wrong-but-fast reply).

## Notes
