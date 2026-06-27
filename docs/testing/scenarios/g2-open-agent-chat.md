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
**Result: BLOCKED**
**Tested: 2026-06-24 12:04 UTC**

The test scenario was blocked at Step 1 due to UI navigation and feature implementation issues:

**Observations:**
- App is past onboarding and shows the Settings page with the Agent button visible
- The agent.open button (identifier "agent.open", label "Open Agent") is accessible in the UI snapshot but appears blocked or non-functional
- When the agent.open button is tapped while the Settings modal is open, the tap registers as successful but no navigation to the agent chat occurs
- The Settings modal does not close via the Dismiss button (xmark/e56), preventing clear access to other UI areas
- The agent chat interface did not open on any tap attempt of the agent.open affordance

**Blocking Issues:**
1. **Settings Modal Persistence**: The Settings modal appears to be a persistent overlay that doesn't dismiss properly, obscuring navigation
2. **Agent Feature Not Functional**: The agent.open button is non-responsive despite being detected in the accessibility tree
3. **Missing Build Configuration**: The scenario mentions requiring `--UITestAgentStub` flag for testing, but the current app build does not appear to have this flag enabled

**Acceptance Criteria Met: NONE (0/4)**
- Agent chat opens: NO — feature blocked
- Composer working with Send gate: NO — feature not reached
- Message reply functioning: NO — feature not reached
- History and export: NO — feature not reached

**Next Steps:**
- Rebuild app with `--UITestAgentStub` flag as mentioned in scenario prerequisites
- Verify Settings modal is properly dismissible or redesign navigation flow
- Check agent feature implementation status in codebase
