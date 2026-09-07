# Project Research Summary

**Project:** Pod0 — Voice-to-Rust Agent Cutover
**Domain:** Native iOS voice UI cutover onto an existing Rust-owned durable conversation authority, plus new headless Rust host crates (async HTTP provider adapters) entering an existing UniFFI-exposed workspace
**Researched:** 2026-08-22
**Confidence:** HIGH

## Executive Summary

This is not a greenfield build — it is closing the last open seam in an architecture that is already fully proven for text. `SharedAgentConversationSession` is the one durable, Rust-owned conversation authority; `CoreAgentHost` already drives model calls, approvals, and capability execution through it. Voice Mode's UI (`AudioConversationManager`, STT, TTS, barge-in detection) is fully built and already speaks a narrow, frozen protocol (`VoiceTurnDelegate`) — but nothing conforms to it. The entire deliverable of #142 is one new adapter type (`VoiceAgentSessionAdapter`) that translates that protocol onto `SharedAgentConversationSession`, plus finishing and committing six already-drafted-but-uncommitted headless Rust host crates that let this be validated outside the iOS simulator.

The recommended approach is additive, not architectural: keep the facade synchronous (do not adopt UniFFI's async FFI — Swift 6 `Sendable` conformance for it is unresolved upstream), keep retry/backoff ownership in the durable effect outbox rather than the HTTP client, and keep TTS/STT/AVAudioSession entirely native. The only genuinely new code is the adapter itself, an extension to `AudioSessionCoordinatorProtocol` for interruption/route-change delivery (currently has zero surface for this), and parity fixes to the headless host crates (unpooled/duplicate HTTP clients, auto-denying approvals, multiple tokio runtimes) so they validate the same state machine iOS actually runs.

The dominant risk, repeated across all four research files, is a second conversation authority emerging silently through the cancellation/barge-in path rather than the happy path — voice already has local interruption handling, and it is tempting to keep using it for "just the interrupt case" instead of routing barge-in through Rust and waiting for its acknowledgment. This is a correctness constraint with a real latency budget (roughly 100ms end-to-end for barge-in to feel responsive), not a UX nicety, and it must be tested with barge-in injected before-dispatch, mid-stream, and after a side effect has already committed — not just the clean-cancel case.

## Key Findings

### Recommended Stack

The workspace already implements the correct 2025/2026 shape for this problem and should be extended, not replaced. `reqwest =0.12.28`, `tokio =1.53.1`, `uniffi =0.32.0`, and `serde`/`serde_json` stay pinned at their current exact versions — no upgrades needed or wanted for this milestone. The one real gap is observability: add `tracing`/`tracing-subscriber` to `pod0-live-hosts` and `pod0-tts-host`, instrumented per-call with request/cancellation correlation IDs (never the API key or raw body), with a subscriber installed only in `pod0-cli` (the facade itself stays synchronous and never logs).

**Core technologies:**
- `reqwest =0.12.28` (pinned): async HTTP client for provider adapters — already client-reused correctly in `pod0-live-hosts`/`pod0-tts-host`/`pod0-portable-media`; the one gap is `pod0-cli::HostExecutor` building a second, redundant blocking client instead of routing through the already-injected `LiveHosts` instance.
- `tokio =1.53.1` (pinned): async runtime — correct as-is, but must be a single shared runtime instance across all six new host crates, not one per crate (see Critical Pitfalls).
- `uniffi =0.32.0` (pinned), synchronous facade only: durable work crosses FFI as typed, leased, pollable state (lease/poll/effect-outbox), not as async futures — Swift 6 `Sendable` conformance for UniFFI async exports is unresolved upstream, so this pattern is deliberate, not a placeholder.
- `Retryability` (existing, `pod0-application`) + the SQLite effect outbox: retry timing is a durable-core decision, not an HTTP-client concern — do not add `reqwest-middleware`/`reqwest-retry` client-side backoff, which would create two competing retry authorities.

### Expected Features

The feature landscape is fully derivable from the codebase and the project's own explicit non-goals — this is unusually well-grounded relative to typical feature research.

**Must have (table stakes — #142's stated acceptance criteria):**
- `VoiceTurnDelegate` adapter over `SharedAgentConversationSession` — the core deliverable; protocol and target class both already exist and are stable.
- Barge-in sends `CancelAgentTurn` to Rust, not just a local TTS task cancel — today `interruptCurrentSpeech()` only stops local playback.
- Turn exclusivity shared with text via `SharedAgentConversationSession.canSend`.
- Approval-required stage (`AgentTurnStage.approvalRequired`) surfaced through a real, non-bypassing voice approval presenter — no silent auto-approve for hands-free mode.
- Terminal failure stages (`blocked`/`outcomeAmbiguous`/`failed`) surfaced as thrown/finished voice events.
- Conversation resume across relaunch, reusing the same `SharedAgentConversationSession`/conversation ID.
- `AudioSessionCoordinatorProtocol` extended with interruption/route-change delivery — currently has no such surface at all (hard blocker for the physical-device test matrix).
- AVAudioSession mode chosen to keep AirPlay testable (`.voiceChat` mode disables AirPlay outright — avoid it).
- Siri/Shortcuts routing re-enabled last, only after the above pass on physical hardware.

**Should have (differentiators, add after validation):**
- Sentence-boundary TTS chunking on top of `streamingContent` for lower perceived latency.
- Richer tool-invocation captions (progress/duration) — pure Swift-side polish.

**Defer / anti-features (explicitly out of scope for #142):**
- A second/local conversation or transcript store for voice — explicit PROJECT.md non-goal; reintroduces the exact dual-writer bug class this migration exists to close.
- A Swift-side tool dispatcher or approval bypass for voice — reuse `CoreAgentApprovalPresenting` with a spoken-prompt UI, never silent auto-authorization.
- Inventing a `source: .voiceMessage` field — verified absent from `StartAgentTurn`/`AgentTurnProjection`/storage codecs; the TODO comment referencing it is stale/aspirational.
- Moving STT/TTS/`AVAudioSession` ownership into Rust, or adopting a full realtime speech-to-speech model — structurally incompatible with "Rust owns durable decisions, native executes platform primitives."

### Architecture Approach

Voice reuses the exact request/response and cancellation pattern text already validated: a thin, stateless adapter (`VoiceAgentSessionAdapter`, the only new Swift type) wraps one `SharedAgentConversationSession`, translating `startTurn`/`streamingContent`/`conversation` into the `VoiceTurnDelegate` protocol's `AsyncThrowingStream<VoiceTurnEvent, Error>`, and translating stream cancellation into `session.cancelActiveTurn()`. The adapter owns no durable state — every field it reads is already a projection. On the Rust side, `pod0-cli::HostExecutor` mirrors `CoreAgentHost` (model calls, approval presentation, capability execution) so the identical `pod0-application` actor and `ApplicationCommand`/`HostRequest` cycle can be exercised headlessly, without the simulator.

**Major components:**
1. `VoiceAgentSessionAdapter` (NEW) — conforms `SharedAgentConversationSession` to `VoiceTurnDelegate`; the sole deliverable on the Swift side.
2. `SharedAgentConversationSession` (existing, unmodified) — the one conversation authority for both text and voice; dispatches `ApplicationCommand.startAgentTurn`/`.cancelAgentTurn` and republishes `ProjectionEnvelope` via `@Observable`.
3. `Pod0Facade` / UniFFI boundary (existing, unmodified) — synchronous; cancellation uses `expected_turn_revision` as an optimistic-concurrency fence, rejecting a cancel if Rust's authoritative revision has already advanced.
4. `pod0-cli::HostExecutor` + `pod0-live-hosts` (uncommitted, needs finishing) — headless mirror of `CoreAgentHost`, currently auto-denies approvals and lacks capability execution; must reach behavioral parity before headless tests can validate #142's approval/cancellation acceptance criteria.

Key invariant to preserve: partial streamed text (`CoreAgentStreamingState`) is presentation-only and is cleared on every terminal stage — never treat the last-seen partial as authoritative on cancel or failure; fall back to what the durable projection actually committed.

### Critical Pitfalls

1. **Second conversation authority emerges through the cancellation path, not the happy path** — voice already has local interruption handling, making it tempting to keep barge-in Swift-local instead of routing it through Rust. Avoid by treating barge-in as a command sent to `SharedAgentConversationSession` (client mutes optimistically, but doesn't consider the turn cancelled until Rust acks) — this is the highest-risk part of #142 per the project's own risk framing.
2. **Cancellation latency treated as UX polish instead of a correctness constraint** — barge-in needs roughly a 100ms budget end-to-end; a slow cancel lets an in-flight call commit a paid/durable side effect before the user's interrupt lands. Avoid with an explicit "cancellable vs. already-committed" state in the turn model, and tests that inject barge-in before-dispatch, mid-stream, and after commit.
3. **Provider HTTP clients without pooling/timeouts/bounded retries turn a transient blip into a stuck voice turn** — voice is the first caller with a hard latency budget. Fix: one shared `reqwest::Client` per process/provider with explicit timeout and pool settings, retries classified via `ProviderError` with exponential backoff + jitter.
4. **Multiple tokio runtimes across the six new host crates cause FFI-concurrency-only deadlocks/panics** — each crate built standalone plausibly owns its own runtime; only breaks once linked together behind one `Pod0Facade` process. Fix: exactly one shared `tokio::runtime::Runtime`, audited via grep before merge.
5. **New crates pass `cargo build`/`test` per-crate but were never exercised together, in CI, or through the facade concurrently** — six crates (1,400+ lines) are currently untracked and outside workspace CI. Fix: `cargo build/test/clippy --workspace --all-targets` as one CI job in the same commit that adds the crates, plus a concurrent-call test against `pending_host_effects`/`next_host_effect_at`.

## Implications for Roadmap

Based on combined research, the natural phase structure follows the dependency graph FEATURES.md and ARCHITECTURE.md both independently converge on, and matches the build order ARCHITECTURE.md recommends directly from the codebase.

### Phase 1: Headless host crates — commit, harden, and gate in CI
**Rationale:** Everything else (adapter development, cancellation-race testing, barge-in latency validation) is cheaper and faster to prove against a real `pod0-application` actor over NDJSON/REPL than against the simulator. This phase is also independently required by PROJECT.md's Active requirements, not just a #142 convenience.
**Addresses:** Stack's client-reuse/observability findings; FEATURES.md's implicit prerequisite for headless validation.
**Avoids:** Pitfalls 3 (unpooled/untimed HTTP clients), 4 (multi-runtime FFI deadlocks), 5 (untested-together crates), 6 (bootstrap/effect-outbox signature risk).

### Phase 2: `VoiceAgentSessionAdapter` — the core #142 deliverable
**Rationale:** Both the protocol (`VoiceTurnDelegate`) and the target (`SharedAgentConversationSession`) already exist and are stable; this can be built and unit-tested in isolation from the audio stack, per ARCHITECTURE.md's build order.
**Delivers:** One new Swift type conforming `SharedAgentConversationSession` to `VoiceTurnDelegate`, reactive (via `withObservationTracking`, not polling) over `streamingContent`/`conversation`.
**Addresses:** FEATURES.md P1 items — adapter itself, turn exclusivity via `canSend`, terminal-failure surfacing, relaunch resume.
**Implements:** ARCHITECTURE.md Pattern 1 (delegate-conformance adapter, no new store) and Pattern 3 (partial text is presentation-only, never committed).

### Phase 3: Barge-in cancellation contract, wired to Rust
**Rationale:** Highest-risk part of the whole project per PITFALLS.md; must be built and verified as its own gated unit before it's buried inside general adapter polish.
**Delivers:** `interruptCurrentSpeech()` calling the adapter's cancel path → `runtime.execute(.cancelAgentTurn(turnId, expectedTurnRevision))`; reject-cancel path with a defined resume behavior.
**Addresses:** FEATURES.md's barge-in-cancels-Rust-turn requirement.
**Avoids:** Pitfalls 1 (second authority via cancellation side-channel) and 2 (latency-as-correctness), verified with barge-in injected at pre-dispatch/mid-stream/post-commit points, not just the clean-cancel case.

### Phase 4: Approval surfaced to voice
**Rationale:** Fans out from the same adapter contract as Phase 2/3 (same `AgentTurnStage`/`HostRequest` mapping), but needs a voice-specific presenter UI decision, so it's called out separately for planning purposes.
**Delivers:** A `CoreAgentApprovalPresenting` implementation for voice — spoken prompt + explicit confirm, never silent auto-approval.
**Addresses:** FEATURES.md's approval-required-turns-surfaced-to-voice requirement; explicit anti-feature guard against an approval bypass.

### Phase 5: Audio session interruption/route-change surface
**Rationale:** Independent of the agent-conversation wiring — a pure `AudioSessionCoordinatorProtocol` gap — so it can be built in parallel with Phases 2–4, but must land before the physical-device matrix can even be attempted.
**Delivers:** Interruption/route-change delivery surface (currently zero methods for this); AVAudioSession mode selection that keeps AirPlay testable.
**Addresses:** FEATURES.md's physical-device interruption/Bluetooth/AirPlay/lock/background matrix requirement.

### Phase 6: Siri/Shortcuts re-enablement (final gate)
**Rationale:** Explicitly sequenced last by both PROJECT.md and the live `VoiceAgentReachabilityTests` guard; depends on every prior phase being stable on physical hardware.
**Delivers:** `VoiceAgentReachabilityTests` guard flipped; cold/warm Siri/AppShortcut invocation re-enabled.
**Addresses:** FEATURES.md's Siri/AppShortcut routing requirement.
**Avoids:** PITFALLS.md's UX pitfall of re-enabling Siri before cold/warm tests are green.

### Phase Ordering Rationale

- Headless crates come first because they're a cheaper, faster substrate for proving cancellation-race and revision-fence behavior than the simulator, and are independently required by PROJECT.md.
- The adapter (Phase 2) precedes cancellation (Phase 3) because barge-in wiring is meaningless without a working turn-submission path first, but both should land within the same milestone slice — PITFALLS.md treats them as one risk unit for verification purposes even though they're staged as two phases here.
- The audio-session interruption surface (Phase 5) is called out separately because ARCHITECTURE.md confirms it shares no components with the adapter/cancellation work and can run in parallel — but FEATURES.md's dependency graph makes it a hard prerequisite for the physical-device matrix, so it cannot be silently dropped or deferred.
- Siri/Shortcuts re-enablement is last because it is the only phase gated by every other phase succeeding on physical hardware, per both PROJECT.md and a live test guard already in the codebase.
- Physical-device playback validation (#84) is explicitly independent of this entire chain (different subsystem — `pod0-portable-media`/native playback routing) and can proceed in parallel with any of the above, per ARCHITECTURE.md's build order note.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1 (headless host crates):** tokio runtime consolidation strategy across six crates and the exact CI job shape need a concrete audit (`grep -rn "Runtime::new\|#[tokio::main]"`) before planning locks in a design — PITFALLS.md flags this as invisible in any single crate's own tests.
- **Phase 3 (barge-in cancellation):** the exact state machine for "cancellable vs. already-committed" inside the Rust turn model isn't fully specified yet — this needs its own design pass during phase planning, not just an acceptance-criteria checklist.

Phases with standard patterns (skip research-phase):
- **Phase 2 (adapter):** shape is fully specified by ARCHITECTURE.md Pattern 1 with example code; protocol and target class are stable and unchanged.
- **Phase 5 (audio session surface):** well-documented Apple platform pattern (`AVAudioSession` interruption/route-change notifications); ARCHITECTURE.md and FEATURES.md both cite the exact API gap.
- **Phase 6 (Siri re-enablement):** gating logic already exists as a live test (`VoiceAgentReachabilityTests`); this is a flip-the-guard step once prior phases are green, not new design.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Verified directly against crates.io, official UniFFI docs, and the actual pinned versions/patterns already in this workspace — not generic advice. |
| Features | HIGH (codebase); MEDIUM (industry practice) | Table-stakes/anti-features are grounded in direct grep/read of the existing Swift and Rust source plus PROJECT.md's explicit non-goals; only the differentiator framing (sentence-boundary chunking, realtime-model trend) leans on external industry sources. |
| Architecture | HIGH (existing components); MEDIUM (new adapter shape) | Component boundaries and data flow are observed facts from the committed Text Agent implementation; the adapter's internal shape is a design recommendation since the type doesn't exist yet. |
| Pitfalls | HIGH (project-specific severity); MEDIUM (general patterns) | Severity and phase mapping are grounded directly in PROJECT.md and CONCERNS.md; the underlying technical patterns (tokio runtime pitfalls, retry jitter, cancellation-tax) are cross-checked across multiple independent external sources. |

**Overall confidence:** HIGH

### Gaps to Address

- The exact "cancellable vs. already-committed" state machine for in-flight turns (Pitfall 2) is not yet designed — needs its own spec during Phase 3 planning, not just acceptance tests.
- `pod0-cli::HostExecutor`'s approval/capability-execution parity gap (auto-deny today) must be closed before headless tests can be trusted for #142's approval acceptance criteria — otherwise scope headless validation to model-turn/cancellation paths only and keep approval-path testing on-device, per ARCHITECTURE.md Anti-Pattern 4.
- Whether `defer_core_wakes` semantics are correct at all four existing effect-outbox call sites (Pitfall 6) needs verification with per-caller behavior tests, not just "compiles."
- No committed research yet on the specific TTS sentence-chunking implementation (deferred to v1.x per FEATURES.md; flagged only as a differentiator, not blocking).

## Sources

### Primary (HIGH confidence)
- Direct codebase inspection across all four research files — `App/Sources/Voice/`, `App/Sources/Core/`, `rust/crates/pod0-application/`, `rust/crates/pod0-facade/`, `rust/crates/pod0-live-hosts/`, `rust/crates/pod0-cli/`, `.planning/PROJECT.md`, `.planning/codebase/CONCERNS.md`, `.planning/codebase/ARCHITECTURE.md`
- [crates.io: reqwest](https://crates.io/crates/reqwest/versions), [crates.io: tokio](https://crates.io/crates/tokio) — version currency checks
- [Mozilla UniFFI: Async Overview](https://mozilla.github.io/uniffi-rs/latest/internals/async-overview.html) — Swift 6 Sendable gap confirmation
- [AVAudioSession routes and AirPlay — Apple](https://developer.apple.com/library/ios/qa/qa1803/_index.html) — `.voiceChat` disables AirPlay
- [Continuous Integration — The Cargo Book](https://doc.rust-lang.org/cargo/guide/continuous-integration.html) — `--workspace --all-targets` requirement

### Secondary (MEDIUM confidence)
- [Voice Agent Interruption Handling — Hamming](https://hamming.ai/resources/voice-agent-interruption-handling-runbook), [Voice AI Barge-In and Turn-Taking: A 2026 Implementation Guide — FutureAGI](https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/) — barge-in latency budgets and industry practice
- [Top 5 Tokio Runtime Mistakes — Techbuddies Studio](https://www.techbuddies.io/2026/03/21/top-5-tokio-runtime-mistakes-that-quietly-kill-your-async-rust/), tokio-rs/tokio Discussions #5840 and #3534 — multi-runtime FFI failure patterns
- [The Cancellation Tax — TianPan.co](https://tianpan.co/blog/2026-04-23-cancellation-tax-streaming-abort-billing) — uncancelled-paid-request risk
- [How to Implement Exponential Backoff with Jitter in Rust — OneUptime](https://oneuptime.com/blog/post/2026-01-25-exponential-backoff-jitter-rust/view) — AWS jitter research citation

### Tertiary (LOW confidence)
- [Sequential Pipeline Architecture for Voice Agents — LiveKit](https://livekit.com/blog/sequential-pipeline-architecture-voice-agents) — corroborating only, not the source of Pod0's design
- [OpenAI voice agents guide](https://developers.openai.com/api/docs/guides/voice-agents), [Real-Time vs Turn-Based Voice Agents in 2026 — Softcery](https://softcery.com/lab/ai-voice-agents-real-time-vs-turn-based-tts-stt-architecture) — realtime speech-to-speech industry trend, used only to justify the anti-feature guard against adopting it

---
*Research completed: 2026-08-22*
*Ready for roadmap: yes*
