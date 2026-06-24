# Scenario G2: Open the agent chat surface

## Goal
Validate opening the agent chat, the composer, conversation history, and starting a
new conversation.

## Prerequisites
- App past onboarding. For UI-only validation, `--UITestAgentStub` makes the agent
  return a canned reply without a live LLM.

## Steps
1. Tap the agent open affordance (`agent.open`, label "Open Agent"). **Expected:**
   Agent chat opens; title "Agent". *Screenshot.*
2. Inspect the composer: text field `agent.input` (placeholder "Message your
   agent…"), a send button ("Send message"). **Expected:** Send is disabled while
   the field is blank. *Screenshot.*
3. Type a message; **Expected:** Send enables. Tap send. With `--UITestAgentStub`
   the reply is "UITestStubReply: agent reply path is working." *Screenshot.*
4. Tap the history button (clock, "Conversation history"). **Expected:** History
   list with the current conversation checkmarked. *Screenshot.*
5. Tap the new-conversation button (plus, "New conversation"). **Expected:** A fresh
   empty conversation (welcome state). *Screenshot.*
6. If messages exist, tap the export button ("Export transcript"). **Expected:** An
   export/share path opens. *Screenshot.*

## Acceptance Criteria
- The agent chat opens with a working composer; Send gates on non-empty input.
- A sent message produces an assistant reply (stub or live).
- History lists conversations and marks the current one; New starts a clean one.
- Export is offered when there are messages.

## Known Issues / Watch Points
- Use `--UITestAgentStub` to decouple from a live LLM for pure UI checks; use a live
  Ollama (G1) for G3/G4.
- `agent.error` identifies an error bubble; a disconnected provider shows the
  disconnected empty state — note which one appears.

## Notes
