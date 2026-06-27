# Scenario L5: Agent creates a contextual highlight (LLM-informed, not a time slice)

## Goal
Validate the flagship claim end-to-end with live inference: ask the agent to clip
the most insightful moment, confirm it runs a `podcast.clip.create` tool action,
that the resulting clip carries the **Agent** source badge, and that the boundaries
are LLM-chosen at idea edges. Extends G4 (Notes empty) with a concrete prompt,
tool-batch verification, and a transcript-grounded boundary judgment.

## Prerequisites
- Live Ollama (L2) + `deepseek-v4-flash:cloud` selected for the Agent role (L1). The
  Agent **Thinking** role may also need a tool-capable model — if the agent never
  calls tools, set the Thinking role too (L1) and retry.
- A transcribed, indexed episode (a Daily episode, per L4/E4).
- NOT the stub.

## Steps
1. Open the agent chat for the transcribed episode (from a transcript segment so the
   episode is in context — L4 step 2). Ask: *"Find the single most insightful moment
   in this episode and create a clip of it."* Tap send. **Expected:** typing
   indicator while the agent reasons over the transcript. *Screenshot.*
2. **Expected:** a tool-batch bubble appears — "Agent ran 1 action" (or "X actions")
   — indicating a `podcast.clip.create` ran. If the agent only describes a moment
   without creating a clip, capture the exchange; it may lack a tool-capable model
   (see Prereqs). *Screenshot.*
3. Open **Clippings**. **Expected:** a new clip with the **Agent** source badge
   (sparkles). Record its time range and title. *Screenshot.*
4. Boundary judgment vs transcript (the headline differentiator): open the clip's
   transcript excerpt and read it. **Expected:** the agent picked a coherent,
   self-contained idea — the excerpt starts and ends on sentence/idea edges and
   reads as "the insightful moment", NOT an arbitrary fixed window. Quote the
   first/last ~6 words and write a one-line judgment (*complete thought? yes/no*).
   Compare the duration to a manual clip from K3 — agent and manual durations should
   differ, reflecting the chosen idea, not a constant. *Screenshot.*
5. Agent-publish contract: per K7, AGENT clips do NOT auto-publish a kind:9802 via
   the user-visible path. Confirm on the host:
   ```
   nak req -k 9802 -a <HEX> -s <T0> wss://relay.primal.net
   ```
   **Expected:** NO kind:9802 event matching this agent clip's range/text (agent
   clips are skipped by `publish_clip_highlight_if_user_visible`). If you WANT it on
   the relay, that requires an explicit share/publish action — note whether the app
   offers one for agent clips. *Document.*

## Acceptance Criteria
- The agent creates a highlight in response to the request, surfaced as a tool-batch
  ("Agent ran N actions").
- The created clip carries the **Agent** (sparkles) source badge.
- The clip's boundaries are semantically meaningful (sentence/idea-aligned),
  documented against the transcript with quoted first/last words.
- The agent clip is NOT auto-published as kind:9802 (matches the K7 contract),
  unless an explicit publish action is taken.

## Known Issues / Watch Points
- This is THE flagship "LLM-informed contextual highlight" claim — heavily document
  boundaries vs content. If the agent's clip cuts mid-sentence or is a blind round
  window, that's a real FAIL worth verbatim Notes.
- If the model can't call tools, it may just describe the moment — capture the full
  transcript of the exchange and check the Thinking role's model (L1).
- A tool batch can be undone; an all-undone batch shows strikethrough "All X actions
  undone" and no clip persists — make sure the clip actually exists in Clippings
  before judging.
- Don't conflate "agent clip not on relay" (correct, per K7) with a publish bug —
  the manual control clips (K2) SHOULD be on the relay in the same window.

## Notes
