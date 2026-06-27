# Scenario L3: Open the agent chat from the root toolbar and round-trip a message

## Goal
Fix G2's BLOCKED navigation: open the agent chat from its REAL entry point — the
**root toolbar** sparkles button (`agent.open`), NOT from inside the Settings modal
(G2 tapped `agent.open` while Settings was open, which is why nothing navigated).
Exercise the composer, send/stop gating, history, and new conversation. Use the
stub first to isolate the UI from the LLM.

## Prerequisites
- App past onboarding.
- For pure UI validation: launch with `--UITestAgentStub` so the agent returns the
  canned reply "UITestStubReply: agent reply path is working." (decouples from
  Ollama). For live behavior, configure Ollama (L2) and skip the stub (covered in
  L4).

## Steps
1. From the ROOT screen (Home/Library tab — NOT inside Settings), locate the
   top-right toolbar sparkles button: accessibilityIdentifier **`agent.open`**,
   label "Open Agent" (keyboard shortcut Cmd+Shift+A). If a Settings modal is open,
   dismiss it FIRST — G2's failure was tapping `agent.open` behind the Settings
   sheet. *Screenshot.*
2. Tap `agent.open`. **Expected:** AgentChatView opens with title "Agent" and either
   a welcome empty state or the prior conversation. *Screenshot.*
3. Inspect the composer: text field accessibilityIdentifier **`agent.input`**
   (placeholder "Message your agent…") and a send button ("Send message").
   **Expected:** Send is disabled while `agent.input` is empty. *Screenshot.*
4. Type into `agent.input`. **Expected:** Send enables. Tap send. With
   `--UITestAgentStub`, the reply is exactly "UITestStubReply: agent reply path is
   working." **Expected:** no `agent.error` bubble appears. *Screenshot.*
5. Tap the history button ("Conversation history", top-left clock). **Expected:** a
   conversation list with the current conversation checkmarked. *Screenshot.*
6. Tap the new-conversation button ("New conversation", top-right plus).
   **Expected:** a fresh empty welcome state. *Screenshot.*
7. If messages exist, tap "Export transcript". **Expected:** an export/share path
   opens. *Screenshot.*

## Acceptance Criteria
- The agent chat opens from the root toolbar `agent.open` button (not blocked by the
  Settings modal).
- Send gates on non-empty `agent.input`; a sent message yields the stub reply with
  no `agent.error`.
- History lists conversations and marks the current; New starts a clean one; Export
  is offered when messages exist.

## Known Issues / Watch Points
- G2's blocker was navigational: `agent.open` was tapped while the Settings modal
  obscured the chat. ALWAYS dismiss Settings first and tap `agent.open` from the
  root toolbar.
- `agent.error` marks an error bubble; a disconnected provider shows the
  disconnected empty state. With the stub, neither should appear — if `agent.error`
  shows under the stub, that's a real FAIL.
- The stub bypasses the LLM entirely; do NOT use it for L4/L5 (those need live
  Ollama).

## Notes
