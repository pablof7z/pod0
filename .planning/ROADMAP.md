# Roadmap: Pod0 — Voice-to-Rust Agent Cutover

## Overview

Voice Mode currently falls back to a stub delegate instead of routing through the same Rust-owned `SharedAgentConversationSession` that text Agent conversations already use. This roadmap closes that seam in four phases: first commit and harden the headless Rust host crates that let the conversation state machine be validated outside the iOS simulator; then build the one new Swift adapter type that gives voice the same durable, cancellable, approval-gated conversation as text (including the highest-risk piece — routing barge-in through Rust instead of a local-only cancel); in parallel, extend the audio session with an interruption/route-change surface and validate playback on physical hardware; and finally, only once both of those are proven stable, flip the guard that re-enables Siri/Shortcuts voice-agent routing.

## Phases

**Phase Numbering:**

- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Headless Host Crates** - Six Rust host crates are committed, compile in and out of the workspace, and validate the real approval/capability state machine in one CI job
- [ ] **Phase 2: Voice Conversation Authority** - Voice Mode routes through `SharedAgentConversationSession` via a new adapter, with barge-in cancelling the Rust turn and approvals surfaced through a real voice presenter
- [ ] **Phase 3: Audio Session Interruption & Physical Hardware Validation** - `AudioSessionCoordinatorProtocol` delivers interruption/route-change events, and playback survives real-hardware interruptions
- [ ] **Phase 4: Siri/Shortcuts Re-enablement** - Siri/Shortcuts voice-agent routing is flipped back on, gated on cold/warm invocation tests passing after Phases 2 and 3 are green

## Phase Details

### Phase 1: Headless Host Crates

**Goal**: The six new Rust host crates are committed, hardened, and prove the same `pod0-application` state machine iOS runs — headlessly, in CI, without the simulator.
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: HOST-01, HOST-02, HOST-03, HOST-04, HOST-05
**Success Criteria** (what must be TRUE):

  1. `cargo build/test/clippy --workspace --all-targets` passes in one CI job covering all six crates (`pod0-cli`, `pod0-live-hosts`, `pod0-nostr-host`, `pod0-portable-media`, `pod0-system-hosts`, `pod0-tts-host`)
  2. Each of the six crates also compiles standalone outside the workspace
  3. HTTP provider clients in `pod0-live-hosts`/`pod0-cli` share one pooled `reqwest::Client` per process with explicit timeouts, not duplicate/unpooled clients
  4. All six host crates share exactly one `tokio` runtime instance when linked into the same process
  5. `pod0-cli::HostExecutor` reaches approval and capability-execution parity with `CoreAgentHost` — headless tests exercise real approvals instead of auto-denying

**Plans**: 3/3 plans executed

Plans:
**Wave 1**

- [x] 01-01-PLAN.md — Join all six host crates into the Cargo workspace and pass the existing CI job (HOST-01, HOST-02)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 01-02-PLAN.md — Consolidate to one tokio runtime, one pooled HTTP client, and add tracing instrumentation (HOST-03, HOST-04)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 01-03-PLAN.md — Flip approval to Approve, wire searchPodcastDirectory capability execution, and prove the state machine headlessly (HOST-05)

### Phase 2: Voice Conversation Authority

**Goal**: Voice interactions use the exact same durable, cancellable, Rust-owned agent conversation as text — no `StubVoiceTurnDelegate` fallback, shared turn exclusivity, Rust-acknowledged barge-in, and a real voice approval presenter.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: VOICE-01, VOICE-02, VOICE-03, VOICE-04, VOICE-05, APPROVAL-01
**Success Criteria** (what must be TRUE):

  1. `VoiceAgentSessionAdapter` conforms `SharedAgentConversationSession` to `VoiceTurnDelegate`, `StubVoiceTurnDelegate` is no longer wired in production, and a turn started from voice blocks a concurrent text send via shared `canSend` (and vice versa)
  2. Barge-in sends `CancelAgentTurn` (with `expected_turn_revision`) to Rust; injected before-dispatch, mid-stream, and after a side effect has already committed, no zombie turn continues and cancellation lands within the ~100ms budget
  3. Terminal failure stages (`blocked`, `outcomeAmbiguous`, `failed`) surface as thrown/finished voice events
  4. Voice conversation resumes the same `SharedAgentConversationSession`/conversation ID across app relaunch (process-death test passes)
  5. Approval-required turns (`AgentTurnStage.approvalRequired`) are surfaced through a real `CoreAgentApprovalPresenting` voice implementation — spoken prompt plus explicit confirm, no silent auto-approve

**Plans**: TBD

Plans:

- [ ] 02-01: TBD

### Phase 3: Audio Session Interruption & Physical Hardware Validation

**Goal**: Voice Mode's audio session correctly delivers interruption/route-change events and playback survives real-world interruptions on physical hardware.
**Mode:** mvp
**Depends on**: Nothing (parallelizable with Phase 2 — different subsystem)
**Requirements**: AUDIO-01, AUDIO-02, AUDIO-03
**Success Criteria** (what must be TRUE):

  1. `AudioSessionCoordinatorProtocol` exposes an interruption/route-change delivery surface (phone-call interruption, Bluetooth/AirPlay route change, backgrounding)
  2. Voice Mode's `AVAudioSession` mode/options keep AirPlay testable (`AVAudioSessionModeVoiceChat` is not used)
  3. On physical hardware, wired disconnect, Bluetooth disconnect/reconnect, phone/Siri interruption resume, and lock-screen controls all behave correctly

**Plans**: TBD
**UI hint**: yes

Plans:

- [ ] 03-01: TBD

### Phase 4: Siri/Shortcuts Re-enablement

**Goal**: Siri/Shortcuts voice-agent routing is re-enabled, but only after the underlying voice conversation and audio stack are proven stable on physical hardware.
**Mode:** mvp
**Depends on**: Phase 2, Phase 3
**Requirements**: SIRI-01
**Success Criteria** (what must be TRUE):

  1. `VoiceAgentReachabilityTests` guard is flipped from force-disabled to enabled
  2. Cold invocation via Siri/Shortcut successfully starts a voice agent turn
  3. Warm invocation (app already running) via Siri/Shortcut successfully starts a voice agent turn
  4. Routing is enabled only once Phase 2 and Phase 3 are verified green

**Plans**: TBD

Plans:

- [ ] 04-01: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 (Phase 3 may run in parallel with Phase 2; Phase 4 requires both complete)

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Headless Host Crates | 3/3 | In Progress|  |
| 2. Voice Conversation Authority | 0/TBD | Not started | - |
| 3. Audio Session Interruption & Physical Hardware Validation | 0/TBD | Not started | - |
| 4. Siri/Shortcuts Re-enablement | 0/TBD | Not started | - |
