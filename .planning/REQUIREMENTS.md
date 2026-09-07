# Requirements: Pod0 — Voice-to-Rust Agent Cutover

**Defined:** 2026-08-22
**Core Value:** Voice interactions use the exact same durable, cancellable, Rust-owned agent conversation as text — no `StubVoiceTurnDelegate` fallback, no second conversation authority, no lost or duplicated turns.

## v1 Requirements

Requirements for closing GitHub issue #142 (Voice→Rust agent cutover) and #84 (physical-device playback validation). Each maps to a roadmap phase.

### Headless Host Crates

- [x] **HOST-02**: All host crates build, test, and lint together in one CI job (`cargo build/test/clippy --workspace --all-targets`), not only per-crate in isolation
- [x] **HOST-03**: HTTP provider clients (`pod0-live-hosts`, `pod0-cli`) share one pooled `reqwest::Client` per process with explicit timeouts, instead of constructing duplicate/unpooled clients
- [x] **HOST-04**: All six host crates share exactly one `tokio` runtime instance when linked into the same process — no per-crate `Runtime::new`/`#[tokio::main]`
- [x] **HOST-05**: `pod0-cli::HostExecutor` reaches approval and capability-execution parity with the native `CoreAgentHost` (no longer auto-denies approvals), so headless tests validate the real state machine

### Voice Agent Adapter

- [ ] **VOICE-01**: A new `VoiceAgentSessionAdapter` conforms `SharedAgentConversationSession` to the existing `VoiceTurnDelegate` protocol, with no `StubVoiceTurnDelegate` production fallback
- [ ] **VOICE-02**: Turn exclusivity is shared between voice and text via `SharedAgentConversationSession.canSend` — a turn started from voice blocks a concurrent text send and vice versa
- [ ] **VOICE-03**: Terminal failure stages (`blocked`, `outcomeAmbiguous`, `failed`) surface as thrown/finished voice events
- [ ] **VOICE-04**: Voice conversation resumes the same `SharedAgentConversationSession`/conversation ID across app relaunch (process-death test passes)
- [ ] **VOICE-05**: Barge-in sends `CancelAgentTurn` (with `expected_turn_revision`) to Rust instead of only cancelling local TTS playback; verified with barge-in injected before-dispatch, mid-stream, and after a side effect has already committed

### Approval

- [ ] **APPROVAL-01**: Approval-required turns (`AgentTurnStage.approvalRequired`) are surfaced to voice through a real `CoreAgentApprovalPresenting` implementation (spoken prompt + explicit confirm) — no silent auto-approve for hands-free mode

### Audio Session

- [ ] **AUDIO-01**: `AudioSessionCoordinatorProtocol` is extended with an interruption/route-change delivery surface (phone-call interruption, Bluetooth/AirPlay route change, backgrounding)
- [ ] **AUDIO-02**: Voice Mode's `AVAudioSession` mode/options keep AirPlay testable (avoids `AVAudioSessionModeVoiceChat`)
- [ ] **AUDIO-03**: Physical-device playback route/interruption validation passes on real hardware — wired disconnect, Bluetooth disconnect/reconnect, phone/Siri interruption resume, lock-screen controls (#84)

### Siri / Shortcuts

- [ ] **SIRI-01**: `VoiceAgentReachabilityTests` guard is flipped and Siri/Shortcuts voice-agent routing is re-enabled only after cold/warm invocation tests pass, and only once all prior voice/audio requirements are green on physical hardware

## v2 Requirements

Deferred to a future release. Tracked but not in the current roadmap.

### Voice Polish

- **VOICEX-01**: Sentence-boundary TTS chunking on top of `streamingContent` for lower perceived latency
- **VOICEX-02**: Richer tool-invocation captions (progress/duration)

### Provenance

- **VOICEX-03**: Voice-specific turn provenance tagging for Run History (requires an ADR-gated Rust contract change; out of scope for #142)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Android native app (M6) | Gated behind an explicit M5 go/hold/stop decision that has not been made |
| M5 Android investment gate evaluation (#61) | Separate evidence-gathering epic, not implementation work in this project |
| Player redesign / CarPlay expansion | Explicit non-goals of #84 |
| Moving STT, TTS, or `AVAudioSession` ownership into Rust | Explicit non-goal of #142; structurally incompatible with "Rust owns durable decisions, native executes platform primitives" |
| A second/local conversation or transcript store for voice | Explicit PROJECT.md non-goal; reintroduces the dual-writer bug class this migration exists to close |
| A Swift-side tool dispatcher or approval bypass for voice | Explicit PROJECT.md non-goal; defeats the approval gate protecting durable state mutations |
| Inventing a `source: .voiceMessage` field on `StartAgentTurn` | Verified absent from the Rust contract; the referencing TODO comment is stale/aspirational — needs its own ADR if ever pursued |
| A full realtime/always-on speech-to-speech model replacing the discrete-turn pipeline | Would re-derive turn boundaries Rust already owns, duplicating the conversation authority |

## Traceability

Populated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| HOST-01 | Phase 1 | Complete |
| HOST-02 | Phase 1 | Complete |
| HOST-03 | Phase 1 | Complete |
| HOST-04 | Phase 1 | Complete |
| HOST-05 | Phase 1 | Complete |
| VOICE-01 | Phase 2 | Pending |
| VOICE-02 | Phase 2 | Pending |
| VOICE-03 | Phase 2 | Pending |
| VOICE-04 | Phase 2 | Pending |
| VOICE-05 | Phase 2 | Pending |
| APPROVAL-01 | Phase 2 | Pending |
| AUDIO-01 | Phase 3 | Pending |
| AUDIO-02 | Phase 3 | Pending |
| AUDIO-03 | Phase 3 | Pending |
| SIRI-01 | Phase 4 | Pending |

**Coverage:**

- v1 requirements: 15 total
- Mapped to phases: 15 (Phase 1: 5, Phase 2: 6, Phase 3: 3, Phase 4: 1)
- Unmapped: 0 ✓

---
*Requirements defined: 2026-08-22*
*Last updated: 2026-08-22 after roadmap creation (traceability populated)*
