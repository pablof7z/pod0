# Pod0 — Voice-to-Rust Agent Cutover

## What This Is

Pod0 is a native iOS podcast app that turns a listener's library into a searchable, conversational knowledge base. Its Rust kernel already owns nearly all durable product logic — library, playback policy, transcripts, workflows, and text-based Agent conversations. This GSD project scopes the final open increment of milestone M4: reconnecting **Voice Mode** to that same Rust-owned agent conversation authority (GitHub issue #142), plus the headless Rust host crates needed to validate that flow outside the iOS simulator, and the remaining physical-hardware playback validation from M1 (#84).

## Core Value

Voice interactions use the exact same durable, cancellable, Rust-owned agent conversation as text — no `StubVoiceTurnDelegate` fallback, no second conversation authority, no lost or duplicated turns.

## Requirements

### Validated

- ✓ Rust kernel is sole authority for library, listening, playback policy, transcripts, chapters, notes, clips, downloads, recall — M2/M3 (closed)
- ✓ Rust owns durable workflows, agent artifacts, permissions, and Nostr coordination — M4, 41/42 issues closed
- ✓ Text Agent conversations run through Rust-owned `SharedAgentConversationSession`
- ✓ One typed UniFFI facade with generated Swift/Kotlin bindings and CI drift detection — M2 (closed)
- ✓ iOS listen-to-recall reliability foundation (truthful workflow status, grounded recall evidence) — M1, 20/22 issues closed

### Active

- [ ] Voice Mode routes through the same Rust-owned `SharedAgentConversationSession` as text Agent, with no `StubVoiceTurnDelegate` production fallback (#142)
- [ ] Voice and text share one durable conversation that survives relaunch; cancellation, barge-in, approval, provider-failure, and process-death tests pass (#142)
- [ ] Siri/Shortcuts voice-agent routing re-enabled only after cold/warm invocation tests pass (#142)
- [ ] New headless Rust host crates (`pod0-cli`, `pod0-live-hosts`, `pod0-nostr-host`, `pod0-portable-media`, `pod0-system-hosts`, `pod0-tts-host`) are committed, compile cleanly in and out of the workspace, and are integrated into CI (supports #142 validation outside the simulator)
- [ ] Physical-device playback route/interruption validation — wired, Bluetooth, Siri interruption, lock-screen controls — on real hardware (#84)

### Out of Scope

- Android native app (M6) — gated behind an explicit M5 go/hold/stop decision that has not been made
- M5 Android investment gate evaluation (#61) — separate evidence-gathering epic, not implementation work
- Player redesign or CarPlay expansion — explicit non-goals of #84
- Moving speech recognition, TTS, or AVAudioSession into Rust — explicit non-goal of #142
- A separate voice conversation store or Swift-side tool dispatcher — explicit non-goal of #142; there is exactly one conversation authority

## Context

- Native Swift 6 / Tuist iOS+iPadOS app (`Podcastr` scheme) with an additive Pod0 Rust kernel (`rust/crates/`: domain, application, storage, facade) linked into iOS via UniFFI.
- Architecture rule: **native executes platform primitives; Rust owns durable product decisions.** Enforced pattern is "one writer per domain" — Swift never writes Rust-owned state back, only renders bounded projections and sends typed commands.
- The GitHub milestone/issue tracker is the authoritative source of current engineering status — verified live via `gh api` on 2026-08-22 (not from `Plans/2026-07-18-ios-first-rust-nmp-roadmap.md`, which is older and partially superseded): M0, M2, M3 fully closed; M1 has 2 open issues (#56 epic, #84); M4 has 1 open issue (#142); M5/M6 are gated and not started.
- The working tree currently has six new, uncommitted Rust crates (`pod0-cli`, `pod0-live-hosts`, `pod0-nostr-host`, `pod0-portable-media`, `pod0-system-hosts`, `pod0-tts-host`) plus `Cargo.toml`/`Cargo.lock` changes (added `reqwest`, `tokio`) — active work-in-progress toward #142's headless validation path.
- Known concerns from the codebase map (`.planning/codebase/CONCERNS.md`, generated 2026-08-22): high unwrap/panic density in `pod0-storage` (3,107 instances), new untested headless methods on `Pod0Facade` (`pending_host_effects`, `next_host_effect_at`), uncommitted external API integrations (OpenAI/Ollama HTTP clients in `pod0-live-hosts`) without visible retry/observability, and effect-outbox query complexity risk.
- Recent commits (`docs: require real headless Pod0 capabilities`, `docs: record Rust-first agent interface decision`) track directly to this scope.

## Constraints

- **Tech stack**: Swift 6 (Xcode 26.6, Tuist 4.200.5), Rust 1.93.0 (pinned via `rust-toolchain.toml`), UniFFI for Swift/Kotlin bindings — fixed by existing architecture, no substitution.
- **Architecture**: one writer per domain; Rust owns durable state and policy, native code executes platform primitives only (AVFoundation, URLSession, Keychain, notifications) — long-term rule, see `README.md` and `.planning/codebase/ARCHITECTURE.md`.
- **Governance**: new or changed Rust ownership requires an ADR plus a migration/deletion link for the replaced Swift owner, per the M0 ownership contract (#55/#64).
- **Typography**: no serif fonts anywhere in the app (`AGENTS.md`).
- **File length**: soft limit 300 lines, hard limit 500 lines (`AGENTS.md`).
- **Android**: explicitly gated behind an M5 go decision — no M6 work in this project's scope.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Scope this GSD project to the current open milestone work (M4 tail #142 + M1 tail #84), not the full M0–M6 arc | The full arc is ~85% complete and already tracked live on GitHub; re-planning it inside GSD would duplicate or drift from the issue tracker | — Pending |
| Keep the research step in the auto pipeline despite #142 already having acceptance criteria and a proposed interface written | User preference, confirmed explicitly during config setup | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-22 after initialization*
