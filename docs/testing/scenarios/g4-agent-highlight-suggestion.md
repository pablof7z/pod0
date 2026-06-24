# Scenario G4: Ask the agent to suggest/create a highlight

## Goal
Validate that the agent can propose and/or create a contextual highlight (NIP-84
clip) from episode content, surfacing as an `.agent`-sourced clip.

## Prerequisites
- Ollama configured + reachable (G1). A transcribed episode (E1/E2).

## Steps
1. Open the agent chat for a transcribed episode. Ask: "Find the most insightful
   moment in this episode and clip it." **Expected:** Typing indicator; the agent
   reasons about the transcript. *Screenshot.*
2. **Expected:** A tool-batch bubble appears ("Agent ran 1 action" / "Agent ran X
   actions") indicating a `podcast.clip.create` action ran. *Screenshot.*
3. Open the **Clippings** tab. **Expected:** A new clip with the **Agent** source
   badge (sparkles) appears. *Screenshot.*
4. Inspect the clip's range vs the transcript (cross-check F2). **Expected:** The
   boundaries align to a coherent idea — contextual, not a fixed window. *Screenshot.*

## Acceptance Criteria
- The agent proposes/creates a highlight in response to the request.
- A created clip carries the `.agent` source badge.
- The clip's boundaries are semantically meaningful (sentence/idea-aligned).
- The tool-batch bubble reports the action(s) the agent ran.

## Known Issues / Watch Points
- This is the flagship "LLM-informed, contextual highlight" claim — the whole point
  is that the agent picks WHAT is meaningful, not a blind time slice. Heavily
  document the boundaries vs. content in Notes.
- The tool batch can be undone; an all-undone batch shows strikethrough "All X
  actions undone".
- If the model declines or can't call tools, capture the transcript of the exchange.

## Notes
