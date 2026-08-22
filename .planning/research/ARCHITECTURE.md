# Architecture Research

**Domain:** Native voice UI ↔ durable cross-platform agent-conversation backend, over a typed FFI boundary
**Researched:** 2026-08-22
**Confidence:** HIGH (component boundaries, data flow, and build order are grounded directly in Pod0's own committed Text Agent implementation and uncommitted headless-host crates, which already implement this exact pattern for text). MEDIUM on the two or three new types #142 must add (they don't exist yet, so the shape below is a design recommendation, not an observed fact). LOW on general industry voice-pipeline claims (single-pass web search, included only to corroborate the cancellation model, not to source the design).

## Standard Architecture

### System Overview

```
┌───────────────────────────────────────────────────────────────────────┐
│  Native Voice UI (existing, unmodified by #142)                       │
│  AudioConversationManager — state machine: idle/listening/thinking/   │
│  speaking/error. Owns STT (SpeechRecognizerService), TTS              │
│  (ElevenLabsTTSClient/AVSpeechFallback), BargeInDetector,              │
│  AudioSessionCoordinator. NEVER talks to Pod0Facade directly.         │
└───────────────────────────┬─────────────────────────────────────────┬─┘
                             │ submitUtterance(text)                   │ cancel via
                             │ -> AsyncThrowingStream<VoiceTurnEvent>  │ Task.cancel()
                             ▼                                         │
┌───────────────────────────────────────────────────────────────────┐ │
│  VoiceTurnDelegate (existing protocol, App/Sources/Voice/)         │ │
│  Narrow contract: submitUtterance, canSubmit. Deliberately the     │ │
│  ONLY thing AudioConversationManager knows about the agent layer.  │ │
└───────────────────────────┬─────────────────────────────────────────┘
                             │ conformance (NEW — this is #142's deliverable)
                             ▼
┌───────────────────────────────────────────────────────────────────────┐
│  VoiceAgentSessionAdapter (NEW — typed native adapter, #142)          │
│  Wraps one SharedAgentConversationSession. Translates:                │
│   - streamingContent / conversation.turns → VoiceTurnEvent stream     │
│   - Task cancellation → session.cancelActiveTurn()                   │
│  Owns NO durable state. Every field it reads is already a projection. │
└───────────────────────────┬─────────────────────────────────────────┘
                             │ startTurn(text) / cancelActiveTurn()
                             ▼
┌───────────────────────────────────────────────────────────────────────┐
│  SharedAgentConversationSession (existing, App/Sources/Core/)         │
│  @MainActor @Observable. Single conversation authority for BOTH text  │
│  and voice. Dispatches ApplicationCommand.startAgentTurn /            │
│  .cancelAgentTurn, subscribes to ProjectionEnvelope, exposes          │
│  conversation: AgentConversationProjection, phase, stateRevision.     │
└───────────────────────────┬─────────────────────────────────────────┘
                             │ ApplicationCommand (typed enum)
                             ▼
┌───────────────────────────────────────────────────────────────────────┐
│  Pod0Facade / SharedLibraryClient (UniFFI boundary)                   │
│  runtime.execute(.startAgentTurn) / .cancelAgentTurn(turnId,          │
│  expectedTurnRevision) — turn revision is the optimistic-concurrency  │
│  fence; cancel is rejected if the turn already moved past the         │
│  revision the caller last observed.                                  │
└───────────────────────────┬─────────────────────────────────────────┘
                             │ HostRequest (ExecuteAgentModelTurn,
                             │  PresentAgentApproval, ExecuteAgentCapability)
                 ┌───────────┴────────────┐
                 ▼                        ▼
┌───────────────────────────┐  ┌─────────────────────────────────────┐
│  Native path (existing)    │  │  Headless path (NEW crates, #142)    │
│  CoreAgentHost              │  │  pod0-cli::HostExecutor              │
│  (App/Sources/Core/         │  │  (rust/crates/pod0-cli/src/host.rs)  │
│  CoreAgentHost.swift)        │  │  delegates model calls to            │
│  - URLSession model calls    │  │  pod0-live-hosts (chat.rs /          │
│  - AgentApprovalCoordinator  │  │  openai_chat.rs / ollama_chat.rs)    │
│    (auto-approve)            │  │                                     │
│  - LiveCoreAgentCapability-  │  │  Runs the SAME pod0-application      │
│    Executor (playback, etc.) │  │  actor + facade, out of simulator,   │
└───────────────────────────┘  │  driven by NDJSON/REPL protocol       │
                                 └─────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Status |
|-----------|----------------|--------|
| `AudioConversationManager` | Owns audio state machine, STT/TTS/barge-in, captions | Exists, out of #142 scope (non-goal: don't touch STT/TTS/AVAudioSession) |
| `VoiceTurnDelegate` protocol | The one seam voice knows about the agent | Exists (`App/Sources/Voice/VoiceTurnDelegate.swift`) |
| `VoiceAgentSessionAdapter` (name TBD) | Conforms `SharedAgentConversationSession` to `VoiceTurnDelegate` | **Does not exist — #142's core deliverable** |
| `SharedAgentConversationSession` | Rust-owned conversation authority: turns, revisions, cancellation, projections | Exists (`App/Sources/Core/SharedAgentConversationSession.swift`), already shared by text |
| `CoreAgentStreamingState` | Transient, keyed by `(turnID, fenceID)`, holds in-flight partial model text | Exists — the adapter reads this, does not own a second copy |
| `CoreAgentHost` | Native `HostRequest` executor: model calls, approval presentation, capability execution | Exists, unmodified by #142 |
| `AgentApprovalCoordinator` | Auto-approves exact Rust-issued proposals (never edits/widens) | Exists, reused as-is for voice |
| `pod0-cli` | Headless shell (NDJSON + REPL) driving the same `pod0-application` actor without iOS | Uncommitted, in progress |
| `pod0-cli::HostExecutor` | Headless `HostRequest` executor mirroring `CoreAgentHost` | Uncommitted — **currently denies approvals and reports capability execution unsupported** (see Gaps) |
| `pod0-live-hosts` | Bounded HTTP/model-provider primitives (chat, embeddings, rerank, download) | Uncommitted, no retry/scheduling policy by design |

## Recommended Project Structure

No new top-level structure is needed — #142 fills one gap in an existing, working layout. The only new source location is the adapter itself:

```
App/Sources/Voice/
├── AudioConversationManager.swift      # unchanged
├── VoiceTurnDelegate.swift             # unchanged — the contract the adapter implements
├── VoiceAgentSessionAdapter.swift      # NEW — the only file #142 must add on the Swift side
└── ...(STT/TTS/barge-in — unchanged)

rust/crates/pod0-cli/                   # already scaffolded, needs finishing + committing
├── src/host.rs                         # extend: approve (not deny) + capability execution stub
├── src/host/agent_http.rs              # already mirrors CoreAgentHost.executeModel
└── src/runner.rs                       # NDJSON/REPL harness already exists (ask_agent, host_drain)
```

### Structure Rationale

- The adapter lives in `App/Sources/Voice/`, not `App/Sources/Core/`, because it is a voice-specific translation, not a durable-state type. `SharedAgentConversationSession` itself stays untouched — this preserves "no separate voice conversation store."
- `pod0-cli` is the validation harness, not a new architecture layer. It exists to prove the same `ApplicationCommand`/`HostRequest` cycle that iOS drives, driven instead by a script or CI job — build order should treat it as a test tool, not a dependency of the adapter.

## Architectural Patterns

### Pattern 1: Delegate-conformance adapter over an existing session (not a new store)

**What:** `VoiceAgentSessionAdapter` holds a reference to one `SharedAgentConversationSession` and conforms it to `VoiceTurnDelegate`. It has no `@Observable` state of its own beyond what it needs to bridge an `AsyncThrowingStream` (e.g., a per-utterance continuation and a Combine/Observation-tracking task that watches `session.streamingContent` and `session.conversation`).
**When to use:** Exactly the situation here — a second UI surface (voice) must reuse a single durable session already serving another surface (text) without becoming a second writer.
**Trade-offs:** Requires translating `@Observable` property changes into stream events by hand (Swift's Observation framework has no native "stream of changes" API), typically via `withObservationTracking` recursion or a polling `Task` bridging `content`/`conversation` reads into `.partialText`/`.finalText` yields. This is more code than a callback, but keeps `SharedAgentConversationSession` free of any voice-specific method.

**Example (shape, not final code):**
```swift
@MainActor
final class VoiceAgentSessionAdapter: VoiceTurnDelegate {
    private let session: SharedAgentConversationSession
    var canSubmit: Bool { session.canSend }

    func submitUtterance(_ text: String) -> AsyncThrowingStream<VoiceTurnEvent, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                await session.startTurn(text)
                // observe session.streamingContent / session.activeTurn until terminal,
                // yielding .partialText / .toolInvocation / .finalText, matching the
                // same terminal-stage set CoreAgentStreamingState.clear() already uses.
            }
            continuation.onTermination = { _ in
                task.cancel()
                Task { await session.cancelActiveTurn() }
            }
        }
    }
}
```

### Pattern 2: Turn revision as the optimistic-concurrency fence for cancellation

**What:** `ApplicationCommand.cancelAgentTurn` requires `turn_id` **and** `expected_turn_revision` (`rust/crates/pod0-application/src/contract.rs`). The existing `SharedAgentConversationSession.cancelActiveTurn()` reads `activeTurn.revision` at call time and sends it — if Rust's authoritative revision has since advanced (e.g., a `HostObservation` already committed), the cancel is rejected rather than silently cancelling a turn the caller no longer has an accurate view of.
**When to use:** Any command that mutates a specific turn/session that might have moved since the caller last observed it — this is exactly the barge-in race (user interrupts at the same instant the model finishes).
**Trade-offs:** The adapter must not cache its own copy of the revision; it must always read `session.activeTurn?.revision` at the moment of cancellation, not at the moment `submitUtterance` was called, or it reintroduces a second, stale writer.

### Pattern 3: Partial text is presentation-only, never committed state

**What:** `CoreAgentStreamingState` is explicitly transient and keyed by `(turnID, fenceID)`; it is cleared the instant the turn reaches a terminal stage (`SharedAgentConversationSession.receive(_:)` calls `streamingState.clear(turnID:)` for every terminal turn). The durable `AgentConversationProjection` only ever contains committed messages.
**When to use:** Mapping model or STT streaming deltas to any UI — captions, TTS chunks, or Voice's `liveAgentText`.
**Trade-offs:** None — this is the invariant #142 must preserve, not a choice. The pitfall (see below) is treating the last-seen partial as authoritative if the stream ends abnormally (cancel, provider failure) instead of falling back to whatever the projection says actually committed.

## Data Flow

### Utterance-in / Projection-delta-out Flow

```
User speech (device mic)
    ↓ (native STT — unchanged)
AudioConversationManager.handleFinalUserText()
    ↓ submitUtterance(text)
VoiceAgentSessionAdapter.submitUtterance(text)
    ↓ await session.startTurn(text)
SharedAgentConversationSession.startTurn()
    ↓ runtime.execute(.startAgentTurn(conversationId, userInput, modelReference))
Pod0Facade → pod0-application actor → AgentTurnStage: awaitingModel
    ↓ HostRequest.ExecuteAgentModelTurn
CoreAgentHost.executeModel() → streamingState.update(...) [transient]
    ↓ ProjectionEnvelope (agentConversation) pushed back on every state change
SharedAgentConversationSession.receive(envelope) → conversation, phase updated
    ↓ adapter observes streamingContent + conversation.turns
VoiceAgentSessionAdapter yields .partialText / .finalText / .toolInvocation
    ↓
AudioConversationManager → captions + TTS
```

### Cancellation / Barge-in Flow

```
BargeInDetector.confirmed
    ↓
AudioConversationManager.interruptCurrentSpeech()
    ↓ speakingTask.cancel()  (native TTS task — unaffected by backend state)
    [in parallel, if the agent turn is still running:]
VoiceAgentSessionAdapter's submitUtterance-stream .onTermination
    ↓ task.cancel() + await session.cancelActiveTurn()
SharedAgentConversationSession.cancelActiveTurn()
    ↓ runtime.execute(.cancelAgentTurn(turnId, expectedTurnRevision: activeTurn.revision))
Pod0Facade rejects or applies cancel against the CURRENT revision
    ↓ ProjectionEnvelope reflects AgentTurnStage.cancelled (terminal)
CoreAgentStreamingState.clear(turnID) — partial text is discarded, never spoken
```

### Key Data Flows

1. **Utterance submission never bypasses `SharedAgentConversationSession`.** The adapter is a pure translator; it holds no conversation ID, no message list, and no revision counter of its own — every read goes through `session`.
2. **Cancellation is bidirectional but asymmetric.** Audio-side cancellation (`speakingTask.cancel()`) is instantaneous and local; backend cancellation (`cancelActiveTurn()`) is a network-adjacent round trip through the facade and can fail (turn already advanced) — the adapter's stream must finish cleanly either way rather than blocking `interruptCurrentSpeech()`.
3. **Approvals and capability execution reuse `CoreAgentHost` unchanged.** Voice turns flow through the identical `HostRequest.PresentAgentApproval` / `.ExecuteAgentCapability` path as text turns; there is no voice-specific approval UI to build (non-goal implicitly protected by this shape — approvals stay text-shaped, `AgentApprovalCoordinator` still auto-approves exact proposals).

## Scaling Considerations

Not applicable in the traditional multi-user sense — this is a single-device, single-conversation-at-a-time consumer app. The only "scale" axis that matters:

| Axis | Consideration |
|------|----------------|
| Concurrent turns per conversation | Already bounded by `canSend`/`activeTurn == nil` in `SharedAgentConversationSession` — voice must respect the same guard, not add a second submission path that races it |
| Barge-in latency | Native TTS cancellation must stay local (no FFI round trip) — confirmed by existing `interruptCurrentSpeech()`, which cancels `speakingTask` before it ever awaits the backend |
| Headless test throughput | `pod0-cli` runs one `pod0-application` actor per process — parallelizing CI validation means multiple store paths, not multiple sessions per process |

## Anti-Patterns

### Anti-Pattern 1: A second conversation store for voice

**What people do:** Give `AudioConversationManager` its own turn/message array so it can render captions immediately without waiting on the facade round trip.
**Why it's wrong:** Explicit non-goal in `.planning/PROJECT.md` — "no separate voice conversation store... there is exactly one conversation authority." It also reintroduces exactly the dual-writer bug class M0–M4 spent the whole migration eliminating.
**Do this instead:** Render captions from `CoreAgentStreamingState`/`conversation` via the adapter, same as text does. If perceived latency is a problem, that's a UX/animation concern in `AudioConversationManager`, not a reason to duplicate state.

### Anti-Pattern 2: Treating the last streamed partial as the spoken text on cancel/failure

**What people do:** On turn failure or cancellation, fall back to whatever `liveAgentText` last held, on the theory that "something is better than silence."
**Why it's wrong:** Partial text is explicitly not committed state (Pattern 3 above); speaking it treats a token stream that may not even be grammatically complete, and may be entirely discarded by the model (a fenced retry can produce different final content), as if it were the agent's answer.
**Do this instead:** On non-terminal-success stages (`cancelled`, `blocked`, `outcomeAmbiguous`, `failed`), transition `AudioConversationManager` to `.error(...)` or silently return to listening — never synthesize speech from `CoreAgentStreamingState.content` after the stream has thrown.

### Anti-Pattern 3: Polling the projection from the adapter instead of subscribing

**What people do:** Have the adapter `Timer`-poll `session.conversation` to decide when to yield `.finalText`.
**Why it's wrong:** Already a named anti-pattern in `.planning/codebase/ARCHITECTURE.md` ("Polling from Swift for Rust State") — `SharedAgentConversationSession` already subscribes to `ProjectionEnvelope` and republishes via `@Observable`; polling on top of that is a redundant second observer with its own race window.
**Do this instead:** Use Swift's `withObservationTracking` (or an `AsyncStream` built from repeated tracking registration) over `session.streamingContent` and `session.conversation` to drive the adapter's yields — reactive, not polled.

### Anti-Pattern 4: Letting the headless host silently diverge from the native host's decisions

**What people do:** Leave `pod0-cli`'s `HostExecutor` auto-denying approvals (as it does today — `HostRequest::PresentAgentApproval => AgentApprovalDecision::Deny`, `rust/crates/pod0-cli/src/host.rs:96-103`) and treat that as acceptable because "it's just for testing."
**Why it's wrong:** If the headless host's approval/capability behavior doesn't match `CoreAgentHost`'s (auto-approve, live capability execution), headless tests validate a different state machine than production — they'll pass while native cancellation-with-approval-in-flight paths stay unexercised.
**Do this instead:** Before headless tests are trusted for #142's approval/cancellation acceptance criteria, make `pod0-cli`'s executor approve exact proposals the same way `AgentApprovalCoordinator` does (or make the divergence an explicit, test-only CLI flag) — otherwise scope headless validation to the model-turn/cancellation paths only, and keep approval-path testing on-device.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| Model provider (OpenAI-compatible / Ollama) | `HostRequest.ExecuteAgentModelTurn` → native `URLSession` (`CoreAgentHost`) or headless `reqwest` (`pod0-live-hosts::chat`) | Same contract both paths; response shape validated identically (rejects multi-tool-call responses, empty completions) |
| ElevenLabs TTS | Stays entirely native (`ElevenLabsTTSClient`) per explicit non-goal — `pod0-tts-host` exists for *other* Rust-driven audio (e.g., generated-episode narration), not for the voice-agent turn path | Do not route Voice Mode's speech synthesis through `pod0-tts-host`; that would violate "no moving TTS into Rust" |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| `AudioConversationManager` ↔ `VoiceAgentSessionAdapter` | `VoiceTurnDelegate` protocol (`submitUtterance`, `canSubmit`) | Existing, frozen contract — #142 must not widen it |
| `VoiceAgentSessionAdapter` ↔ `SharedAgentConversationSession` | Direct method calls (`startTurn`, `cancelActiveTurn`) + `@Observable` property reads | New — the entire deliverable of #142 on the Swift side |
| `SharedAgentConversationSession` ↔ Rust kernel | `ApplicationCommand` in, `ProjectionEnvelope` out, via `SharedAgentConversationRuntime` (implemented by `SharedLibraryClient`) | Existing, unchanged — voice is just a second consumer |
| `pod0-cli` ↔ `pod0-application` actor | In-process, same crate graph as `Pod0Facade` | Not a network boundary — it links the same Rust code iOS links, minus UniFFI |
| `pod0-cli::HostExecutor` ↔ `pod0-live-hosts` | Direct Rust calls, blocking `reqwest` client on a dedicated `tokio::runtime::Runtime` | Mirrors, but does not share code with, `CoreAgentHost`'s Swift `URLSession` calls — the two must be kept behaviorally in sync by hand (see Anti-Pattern 4) |

## Suggested Build Order

1. **Finish and commit the headless host crates first, narrowly scoped to the agent-turn path.** They already compile and already implement `ExecuteAgentModelTurn` correctly; the only gap is approval/capability parity (Anti-Pattern 4). This unblocks writing cancellation/turn-revision tests against a real `pod0-application` actor before any Swift code exists, catching contract bugs (e.g., revision-mismatch rejection semantics) at the cheapest layer.
2. **Write `VoiceAgentSessionAdapter` against the existing `SharedAgentConversationSession` and `VoiceTurnDelegate` — no other Swift type needs to change.** `AudioConversationManager` already calls `turnDelegate?.submitUtterance` and already fails closed (`state = .error(...)`) when no delegate is set, so the adapter can be developed and unit-tested in isolation, then wired in via `AudioConversationManager.setTurnDelegate(_:)` at composition time (mirrors how `SharedLibraryClient.makeAgentConversationSession()` already composes the text session).
3. **Validate cancellation-with-turn-revision and barge-in races using the headless crates, not the simulator**, since `pod0-cli`'s REPL already exposes `ask_agent` and `host_drain` — add a `cancel_agent`-equivalent command if missing, then script the exact race (start turn, cancel at varying revisions) as a fast, deterministic test before attempting it live against STT/TTS timing in the simulator.
4. **Re-enable Siri/Shortcuts voice-agent routing last**, only after cold/warm invocation tests pass against the finished adapter — this is explicitly sequenced last in `.planning/PROJECT.md`'s Active requirements and depends on everything above being stable.
5. **Physical-device playback validation (#84) is independent and can proceed in parallel** — it touches `pod0-portable-media`/native `AVAudioSession` playback routing, not the agent conversation path, and shares no components with steps 1–4.

## Sources

- Direct code inspection (HIGH confidence, primary source for all component/data-flow claims above):
  - `App/Sources/Voice/AudioConversationManager.swift`, `VoiceTurnDelegate.swift`
  - `App/Sources/Core/SharedAgentConversationSession.swift`, `SharedAgentConversationRuntime.swift`, `CoreAgentStreamingState.swift`, `CoreAgentHost.swift`, `SharedLibraryClient+Agent.swift`
  - `App/Sources/Agent/AgentApprovalCoordinator.swift`
  - `rust/crates/pod0-application/src/contract.rs` (`ApplicationCommand::StartAgentTurn` / `CancelAgentTurn`)
  - `rust/crates/pod0-cli/src/{lib.rs,host.rs,runner.rs,host/agent_http.rs}`
  - `rust/crates/pod0-live-hosts/src/lib.rs`, `rust/crates/pod0-tts-host/src/lib.rs`, `rust/crates/pod0-portable-media/src/lib.rs`
  - `.planning/PROJECT.md`, `.planning/codebase/ARCHITECTURE.md`
- [Sequential Pipeline Architecture for Voice Agents | LiveKit](https://livekit.com/blog/sequential-pipeline-architecture-voice-agents) — LOW confidence, corroborating only (cancellation-through-pipeline pattern; not the source of Pod0's design, which predates and independently matches it)
- [Configuring Turn Detection and Interruptions in LiveKit Agents | LiveKit](https://livekit.com/blog/turn-detection-and-interruption-handling) — LOW confidence, corroborating only

---
*Architecture research for: Voice-to-Rust agent conversation cutover (#142)*
*Researched: 2026-08-22*
