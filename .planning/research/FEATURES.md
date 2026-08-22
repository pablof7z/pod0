# Feature Research

**Domain:** Voice UI re-plumbed onto an existing durable, cancellable, text-based agent conversation session (Pod0 #142)
**Researched:** 2026-08-22
**Confidence:** HIGH (codebase-grounded); MEDIUM where based on external voice-agent industry practice

## Codebase grounding

Verified directly in the working tree, not from memory:

- `VoiceTurnDelegate` protocol (`App/Sources/Voice/VoiceTurnDelegate.swift`) and `AudioConversationManager` (`App/Sources/Voice/AudioConversationManager.swift`) already exist with a full state machine (idle/listening/thinking/speaking/error), ambient and push-to-talk modes, captions, and a barge-in detector with an "optimistic preview → confirmed" two-stage UX. `grep` for `: VoiceTurnDelegate` finds **zero** conforming types — the adapter to `SharedAgentConversationSession` is unbuilt; this is the actual deliverable of #142.
- `SharedAgentConversationSession` (`App/Sources/Core/SharedAgentConversationSession.swift`) is the one durable conversation authority for text. It exposes `startTurn(_:)`, `cancelActiveTurn()`, `canSend`, `activeTurn`, `streamingContent`, and resumes an existing conversation via `resumeConversationID` in its initializer.
- `VoiceAgentReachabilityTests.swift` currently asserts Voice Mode is **not** mounted (`RootView` has no `VoiceView(`, `AppIntents` has no `StartVoiceModeIntent` / "Talk to my podcasts" strings, and `AudioConversationManager.swift` contains no `StubVoiceTurnDelegate`). This is a live guardrail, not documentation of a shipped stub — re-enabling any of these is gated on the adapter existing and its tests passing.
- Approval flow is real and already used by text: `CoreAgentHost.presentApproval` (`App/Sources/Core/CoreAgentHost.swift`) calls a `CoreAgentApprovalPresenting.requestApproval(_:)` and `AgentTurnStage.approvalRequired` is a real, reachable stage (`rust/crates/pod0-application/src/agent_turn_contract.rs`). No voice-specific approval presenter exists.
- `AudioSessionCoordinatorProtocol` (`App/Sources/Voice/VoiceAudioSessionBridge.swift`) has exactly three methods — `beginVoiceCapture`, `beginVoicePlayback`, `endVoiceSession` — and only a `NoopAudioSessionCoordinator` implementation. **There is no interruption or route-change delivery surface at all** (no callback/stream for phone-call interruption, Bluetooth/AirPlay route change, or backgrounding). This is a hard blocker for the acceptance criterion's physical-device interruption matrix, not just an untested path.
- `interruptCurrentSpeech()` in `AudioConversationManager` cancels the local `speakingTask`/`bargeTask` and stops TTS, but never calls anything equivalent to `SharedAgentConversationSession.cancelActiveTurn()`. Today barge-in is purely local; it does not send `CancelAgentTurn` to Rust. If the adapter doesn't fix this, a barged-in turn keeps running/committing on the Rust side after the user has moved on — the exact "duplicated or lost turns" failure PROJECT.md rules out.
- `CoreAgentStreamingState` flushes partial text on a flat 50ms timer (`App/Sources/Core/CoreAgentStreamingState.swift`) — tuned for a text UI, not for TTS sentence-chunking.
- `rust/crates/pod0-application/src/contract.rs` `StartAgentTurn` has fields `conversation_id`, `user_input`, `model_reference` only — **no source/channel field exists anywhere in the Rust contract** (`AgentTurnProjection`, `StartAgentTurn`, storage codecs all checked). The `TODO(run-logs)` comment in `VoiceTurnDelegate.swift` referencing `source: .voiceMessage` and a non-existent `AgentChatSession.startSend(...)` API is stale/aspirational — that class and method do not exist; the real target type is `SharedAgentConversationSession.startTurn(_:)`, which takes no source parameter.

## Feature Landscape

### Table Stakes (Users Expect These)

Features required for the cutover to be genuinely done — matches #142's stated acceptance criteria.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `VoiceTurnDelegate` adapter over `SharedAgentConversationSession` | The entire point of #142; without it Voice Mode has no delegate to inject except a stub | MEDIUM | Protocol is already defined and stable; adapter must map `startTurn`/`streamingContent`/`conversation` into `submitUtterance` → `AsyncThrowingStream<VoiceTurnEvent, Error>` |
| Barge-in cancels the Rust-side turn, not just local TTS | Acceptance criteria explicitly require barge-in tests to pass; a merely-local cancel leaves a zombie turn racing the next one on the one shared conversation | MEDIUM | `interruptCurrentSpeech()` must call the adapter's cancel path, which must call `runtime.execute(.cancelAgentTurn(...))` — this wiring does not exist today |
| Turn exclusivity shared between text and voice | One conversation authority means a turn started from voice must block a concurrent text send and vice versa | LOW | `SharedAgentConversationSession.canSend` already encodes this gate; the adapter must read it (via `canSubmit`) rather than tracking its own "can I send" bit |
| Approval-required turns surfaced to voice | `AgentTurnStage.approvalRequired` is reachable from any turn regardless of input channel; silently hanging or auto-denying breaks the shared-conversation guarantee | MEDIUM–HIGH | Needs a `CoreAgentApprovalPresenting` implementation for voice (spoken prompt + explicit confirm, not silent auto-approve) — see anti-feature below on auto-approval |
| Terminal-failure surfacing (`.blocked`, `.outcomeAmbiguous`, `.failed`) as a thrown/finished voice event | Acceptance criteria require provider-failure tests to pass; today `VoiceTurnEvent` only carries partial/final text/tool-invocation, no explicit failure-stage mapping | LOW–MEDIUM | Map terminal failure stages to the stream throwing, matching the doc contract already written in `VoiceTurnDelegate.swift` |
| Relaunch resumes the same conversation for voice | Acceptance criteria: "voice and text share one durable conversation that survives relaunch"; process-death test must pass | LOW | `SharedAgentConversationSession` already supports `resumeConversationID`; the adapter must be constructed against the same persisted session/ID Voice Mode reuses, not a fresh one |
| AVAudioSession interruption/route-change delivery into the state machine | Acceptance criteria require a physical-device matrix: interruption, Bluetooth, AirPlay, lock/unlock, background | HIGH | `AudioSessionCoordinatorProtocol` has no such surface today — must be extended (e.g. an interruption event stream) before this matrix is even testable, independent of the agent-conversation wiring |
| AVAudioSession mode compatible with AirPlay | `AVAudioSessionModeVoiceChat` disables AirPlay outright (Apple platform behavior) — picking it for echo-cancelled barge-in would make the AirPlay leg of the acceptance matrix permanently fail | LOW | Prefer `.default`/`.spokenAudio`-style modes with `.allowBluetooth`/`.allowBluetoothA2DP`/`.defaultToSpeaker` options over `.voiceChat` |
| AppShortcut/Siri routing gated behind the above | PROJECT.md and `VoiceAgentReachabilityTests` both encode this ordering explicitly | LOW (once the rest exists) | Do not flip the reachability guard until cold/warm invocation tests pass — matches existing test intent |

### Differentiators (Competitive Advantage)

Valuable, not required to call #142 done. Defer without blocking the cutover.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Sentence-boundary TTS chunking | Lets TTS start speaking before the full LLM answer streams in, cutting perceived latency — industry practice for barge-in-capable voice agents targets sub-150ms turn-taking budgets | MEDIUM | `CoreAgentStreamingState`'s flat 50ms flush is text-tuned; a sentence splitter sitting on top of `streamingContent` would feed `.partialText` in speakable units without touching the durable contract |
| Richer tool-invocation captions (progress/duration) | `.toolInvocation` already surfaces a "Running X…" caption; richer status is a UX polish, not correctness | LOW | Pure Swift-side presentation change |
| Preserve/tune the existing ambient "optimistic preview" barge-in UX | Already implemented (`isUserBargingIn`, two-stage barge event) — worth keeping and tuning latency, not rebuilding | LOW | Not a new feature; a tuning pass once real barge-in-cancels-turn is wired |
| Voice-specific turn provenance for Run History | Would let analytics distinguish voice vs typed turns | MEDIUM–HIGH | Requires a new field across `StartAgentTurn`/`AgentTurnProjection`/storage codecs plus an ADR (governance constraint in PROJECT.md) — real work, but not gating #142's stated acceptance criteria, which say nothing about Run History tagging |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|------------------|-------------|
| A second/local conversation or transcript store for voice | Feels simpler to buffer voice messages locally for snappy UI | PROJECT.md explicit non-goal: "a separate voice conversation store... there is exactly one conversation authority"; duplicated state is exactly the "lost or duplicated turns" failure mode #142 exists to close | Render captions directly from `SharedAgentConversationSession.conversation`/`streamingContent`, same as text |
| A Swift-side tool dispatcher or approval bypass for voice | Hands-free mode has no screen to show a confirmation sheet, tempting an auto-approve shortcut | PROJECT.md explicit non-goal ("no separate Swift-side tool dispatcher"); auto-approving proposals defeats the approval gate that protects durable state mutations for every other input channel | Reuse `CoreAgentApprovalPresenting` with a voice-appropriate UI (spoken prompt + explicit confirm gesture/utterance), never silent auto-authorization |
| Inventing a `source: .voiceMessage` tag before the Rust contract supports it | The stale TODO comment in `VoiceTurnDelegate.swift` already suggests it | Verified: no such field exists in `StartAgentTurn`, `AgentTurnProjection`, or storage codecs. Faking it client-side (e.g. a local heuristic or a comment-only convention) creates drift from "one writer per domain" and produces data nobody on the Rust side ever asked for or validates | Either drop the TODO as aspirational-and-premature, or file it as its own ADR-gated Rust contract change outside #142's scope |
| Moving STT, TTS, or `AVAudioSession` ownership into Rust | Server-side speech-to-speech architectures (OpenAI Realtime API, Deepgram Voice Agent API) increasingly make the model own turn detection and audio I/O end-to-end | PROJECT.md explicit non-goal; also structurally incompatible with "Rust owns durable product decisions, native executes platform primitives" — a speech-to-speech model owning turn state would itself become a second conversation authority | Keep STT → shared-conversation-turn → TTS as three native-executed stages around the one Rust-owned turn, as already scaffolded |
| A full realtime/always-on speech-to-speech model replacing the discrete-turn pipeline | Matches where the broader industry is heading in 2026 (OpenAI/Deepgram realtime voice APIs) | Discrete STT→turn→TTS is what the existing `AudioConversationManager` state machine and the one-conversation-authority architecture assume; adopting a continuous speech-to-speech model would require re-deriving turn boundaries Rust already owns, duplicating that authority | Stay with discrete turns bridged through `VoiceTurnDelegate`; revisit only as a deliberate, separately-scoped architecture change |

## Feature Dependencies

```
VoiceTurnDelegate adapter (over SharedAgentConversationSession)
    └──requires──> none (protocol + target class both exist today)

Barge-in cancels Rust-side turn
    └──requires──> VoiceTurnDelegate adapter exposing a cancel path to interruptCurrentSpeech()

Approval-required turns surfaced to voice
    └──requires──> VoiceTurnDelegate adapter
    └──requires──> a voice CoreAgentApprovalPresenting implementation

Terminal-failure surfacing (.blocked/.outcomeAmbiguous/.failed)
    └──requires──> VoiceTurnDelegate adapter

Physical-device interruption/Bluetooth/AirPlay/lock/background matrix
    └──requires──> AudioSessionCoordinatorProtocol gaining an interruption/route-change surface
    └──requires──> AVAudioSession mode choice that permits AirPlay

AppShortcut/Siri cold/warm routing re-enablement
    └──requires──> VoiceTurnDelegate adapter
    └──requires──> Barge-in-cancels-Rust-turn
    └──requires──> Approval-required turns surfaced to voice
    └──requires──> Terminal-failure surfacing
    └──requires──> Physical-device interruption matrix passing
    └──requires──> VoiceAgentReachabilityTests guard flipped only after the above

Sentence-boundary TTS chunking (differentiator)
    └──enhances──> VoiceTurnDelegate adapter (not required by it)

Voice-specific turn provenance (differentiator)
    └──conflicts──> "no invented source tag" anti-feature — only pursue via an explicit ADR-scoped Rust contract change, not inside #142
```

### Dependency Notes

- **AppShortcut/Siri re-enablement requires everything else:** both PROJECT.md ("re-enabled only after cold/warm invocation tests pass") and the live `VoiceAgentReachabilityTests` guard encode this ordering already — do not treat Siri routing as parallelizable with the adapter work.
- **The interruption/route-change gap is independent of the agent-conversation wiring:** it's a `AudioSessionCoordinatorProtocol` surface problem, not a `SharedAgentConversationSession` problem. It can be built in parallel with the adapter but must land before the physical-device matrix in the acceptance criteria can even be attempted.
- **Approval and terminal-failure surfacing both fan out from the same adapter contract:** both are just additional cases the `submitUtterance` stream must map from `AgentTurnStage`/`HostRequest`, so they're naturally built together with the base adapter rather than as separate phases.

## MVP Definition

### Launch With (v1 — required to close #142)

- [ ] `VoiceTurnDelegate` adapter over `SharedAgentConversationSession` — the core deliverable
- [ ] Barge-in sends `CancelAgentTurn` to Rust, not just a local task cancel
- [ ] Turn exclusivity shared with text via `canSend`/`canSubmit`
- [ ] Approval-required stage surfaced through a real (non-bypassing) voice approval presenter
- [ ] Terminal failure stages (`blocked`/`outcomeAmbiguous`/`failed`) surfaced as thrown/finished voice events
- [ ] Conversation resume across relaunch reuses the same `SharedAgentConversationSession`/conversation ID
- [ ] `AudioSessionCoordinatorProtocol` extended with interruption/route-change delivery
- [ ] AVAudioSession mode selected to keep AirPlay testable
- [ ] `VoiceAgentReachabilityTests` guard flipped and Siri/AppShortcut routing re-enabled last, only once the above pass on physical hardware

### Add After Validation (v1.x)

- [ ] Sentence-boundary TTS chunking for lower perceived latency
- [ ] Richer tool-invocation captions

### Future Consideration (v2+)

- [ ] Voice-specific turn provenance in Run History (needs its own ADR-gated Rust contract change)

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| `VoiceTurnDelegate` adapter | HIGH | MEDIUM | P1 |
| Barge-in cancels Rust turn | HIGH | MEDIUM | P1 |
| Approval surfaced to voice | HIGH | MEDIUM-HIGH | P1 |
| Terminal-failure surfacing | HIGH | LOW-MEDIUM | P1 |
| Relaunch resume | HIGH | LOW | P1 |
| Interruption/route-change surface | HIGH | HIGH | P1 |
| AirPlay-safe audio mode | MEDIUM | LOW | P1 |
| Siri/AppShortcut re-enablement | MEDIUM | LOW (given above) | P1 (last) |
| Sentence-boundary TTS chunking | MEDIUM | MEDIUM | P2 |
| Richer tool captions | LOW | LOW | P3 |
| Voice turn provenance tagging | LOW-MEDIUM | MEDIUM-HIGH | P3 |

**Priority key:**
- P1: Must have — part of #142's stated acceptance criteria
- P2: Should have, add when possible
- P3: Nice to have, future consideration / needs separate ADR scope

## Sources

- Direct codebase inspection (HIGH confidence, primary source for this research):
  - `App/Sources/Voice/VoiceTurnDelegate.swift`, `App/Sources/Voice/AudioConversationManager.swift`, `App/Sources/Voice/VoiceAudioSessionBridge.swift`
  - `App/Sources/Core/SharedAgentConversationSession.swift`, `App/Sources/Core/CoreAgentHost.swift`, `App/Sources/Core/CoreAgentStreamingState.swift`
  - `AppTests/Sources/VoiceAgentReachabilityTests.swift`
  - `rust/crates/pod0-application/src/contract.rs`, `rust/crates/pod0-application/src/agent_turn_contract.rs`
  - `.planning/PROJECT.md`
- [Voice Agent Interruption Handling: Barge-In, Backchannels, and Turn Detection](https://hamming.ai/resources/voice-agent-interruption-handling-runbook) — barge-in latency budgets (MEDIUM confidence, industry practice)
- [Voice AI Barge-In and Turn-Taking: A 2026 Implementation Guide](https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/) — conversation-state handling of interrupted utterances (MEDIUM confidence)
- [OpenAI voice agents guide](https://developers.openai.com/api/docs/guides/voice-agents) — realtime/speech-to-speech architecture trend, used to inform the anti-feature note (MEDIUM confidence)
- [Real-Time vs Turn-Based Voice Agents in 2026](https://softcery.com/lab/ai-voice-agents-real-time-vs-turn-based-tts-stt-architecture) — architecture comparison (MEDIUM confidence)
- [AVAudioSession routes and AirPlay](https://developer.apple.com/library/ios/qa/qa1803/_index.html) — confirms `.voiceChat` mode disables AirPlay (HIGH confidence, Apple documentation)
- [Managing Audio Interruption and Route Change in iOS](https://medium.com/@mehsamadi/managing-audio-interruption-and-route-change-in-ios-application-8202801fd72f) — interruption/route-change notification pattern (MEDIUM confidence)

---
*Feature research for: Voice-to-Rust agent conversation cutover (Pod0 #142)*
*Researched: 2026-08-22*
