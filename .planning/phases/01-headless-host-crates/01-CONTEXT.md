# Phase 1: Headless Host Crates - Context

**Gathered:** 2026-08-22
**Status:** Ready for planning

<domain>
## Phase Boundary


</domain>

<decisions>
## Implementation Decisions

Discussed in `--auto` mode: for each gray area, the recommended option (grounded in `.planning/research/` and `.planning/codebase/CONCERNS.md`) was auto-selected. Logged below for audit.

### Credential Handling

  - [auto] Credential Handling — Q: "Keep env-var credentials for `pod0-cli`, or route through the existing keyring integration now?" → Selected: "Keep env vars for this milestone" (recommended default — env vars are the standard headless/CI-friendly pattern; a CI runner has no keyring session to unlock, and the existing README-documented mitigation, "credentials are never returned by the CLI protocol," already applies)

### Observability

- **D-02:** Add `tracing`/`tracing-subscriber` instrumentation to `pod0-live-hosts` and `pod0-tts-host` in this phase (per-call request/cancellation correlation IDs, never the API key or raw body), with a subscriber installed only in `pod0-cli`. The synchronous `Pod0Facade` stays silent — no logging crosses the FFI boundary.
  - [auto] Observability — Q: "Add tracing instrumentation now, in Phase 1, or defer to a later hardening pass?" → Selected: "Now, in this phase" (recommended default — `.planning/research/STACK.md` and `.planning/research/PITFALLS.md` both flag this as closing the exact observability gap `.planning/codebase/CONCERNS.md` raised; it's cheap to add alongside crates already being hardened here rather than shipping untraceable HTTP failures into headless CI runs from day one)

### Tokio Runtime Consolidation

- **D-03:** Exactly one `tokio::runtime::Runtime` is constructed once at the `pod0-cli` binary's entrypoint (and at any future host-process boundary, e.g. an iOS-linked equivalent) and threaded down to host adapters as a `Handle`, rather than each adapter lazily calling `tokio::runtime::Handle::current()`. — **Reversibility:** costly — reworking this after adapters are written against `Handle::current()` would mean touching every host crate's call sites; deciding it now avoids that rework.
  - [auto] Runtime Consolidation — Q: "Single Runtime constructed once and passed down, or each adapter calls `Handle::current()` assuming it always runs inside one?" → Selected: "Single Runtime constructed once, passed down as a Handle" (recommended default — deterministic, matches PITFALLS.md's "audited via grep before merge" guidance, and avoids a panic if a component ever runs outside any runtime context)

### CI Scope

- **D-04:** The existing `cargo deny` job (already run for license/advisory checks per `.planning/codebase/INTEGRATIONS.md`) is confirmed to cover the six new crates and their new dependencies (`reqwest`, `tokio`) as part of this phase — not spun up as new CI infrastructure, just verified in-scope.
  - [auto] CI Scope — Q: "Extend the existing `cargo deny` job's scope to the new crates/deps as part of Phase 1, or treat as a separate follow-up?" → Selected: "In scope for Phase 1" (recommended default — `cargo deny` already runs in the existing pipeline; this just ensures new crates aren't silently excluded from a check that already exists, not new CI work)

### Claude's Discretion

- Exact tracing correlation-ID scheme (span naming, field names) — no user preference stated, follow whatever `tracing` idiom fits the existing `pod0-live-hosts` error types (`ProviderError`, `NetworkError`, `ProtocolError`).
- Whether the single shared `tokio::runtime::Runtime` is multi-thread or current-thread flavored — a Phase 1 planning-level decision informed by the actual concurrency needs of the six crates, not a vision-level choice.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project & Requirements
- `.planning/PROJECT.md` — project scope, constraints, and the "one writer per domain" architecture rule
- `.planning/REQUIREMENTS.md` §Headless Host Crates — HOST-01 through HOST-05, the exact requirements this phase closes
- `.planning/ROADMAP.md` §Phase 1 — goal and success criteria as approved

### Research (grounds every decision above)
- `.planning/research/SUMMARY.md` — executive summary, Phase 1 rationale, and the 5 critical pitfalls this phase must avoid
- `.planning/research/STACK.md` — pinned versions (`reqwest =0.12.28`, `tokio =1.53.1`, `uniffi =0.32.0`), the sync-facade rationale, and the tracing recommendation
- `.planning/research/PITFALLS.md` — pitfalls 3 (unpooled/untimed HTTP clients), 4 (multi-runtime FFI deadlocks), 5 (untested-together crates), 6 (bootstrap/effect-outbox signature risk)
- `.planning/research/ARCHITECTURE.md` — `pod0-cli::HostExecutor`'s approval-parity gap vs. native `CoreAgentHost`, and why it must close before headless tests can validate anything approval-related

### Codebase State
- `.planning/codebase/CONCERNS.md` — the six specific, already-identified gaps in the new crates (unpooled/duplicate HTTP clients, auto-denying approvals, untested headless methods, uncommitted Cargo.lock changes, effect-outbox signature risk, bootstrap atomicity)
- `rust/README.md` — documented credential-handling mitigation for `pod0-cli`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `pod0-application::Retryability` (Never/Automatic/AfterUserAction) — already a durable, UniFFI-exported retry-classification type; adapters should classify failures into it rather than adding client-side retry middleware
- `ProviderError` types in `pod0-live-hosts` (with `retry_after`) — already carry what's needed for the durable effect outbox to own retry timing

### Established Patterns
- Facade stays fully synchronous — durable work crosses FFI via a leased/polled effect-outbox pattern (`pending_host_effects`, `next_leased_headless_host_requests`, `record_leased_host_observation`), never UniFFI async futures (Swift 6 `Sendable` conformance for async FFI is unresolved upstream)
- `pod0-live-hosts`, `pod0-tts-host`, `pod0-portable-media` already build one `reqwest::Client` once and reuse it correctly — `pod0-cli/src/host.rs` is the one place that builds a second, redundant client; fix by routing through the already-injected `LiveHosts` instance instead of adding a new pooling mechanism

### Integration Points
- `pod0-cli::HostExecutor` (`rust/crates/pod0-cli/src/host.rs:96-108`) is the seam where headless approval/capability parity must be closed — mirrors native `CoreAgentHost`
- CI: `.github/workflows/test.yml` is where the new `cargo build/test/clippy --workspace --all-targets` job and the `cargo deny` scope confirmation land

</code_context>

<specifics>
## Specific Ideas

No specific implementation examples were requested beyond what research already grounded — this phase is unusually well-specified going in (existing acceptance criteria on issue #142's dependencies, direct source inspection in all four research files). Follow the research's recommendations as stated rather than inventing alternatives.

</specifics>

<deferred>
## Deferred Ideas

- Full keyring-backed credential storage for `pod0-cli` (replacing env vars) — reversible, not blocking this phase, worth a future ADR if headless credential exposure becomes a real concern
- Voice-specific turn provenance tagging for Run History — explicitly deferred to v2+ per `.planning/REQUIREMENTS.md`, needs its own ADR-gated Rust contract change; unrelated to this phase's Rust-workspace-only scope

### Reviewed Todos (not folded)
None — `todo.match-phase 1` returned zero matches.

</deferred>

---

*Phase: 1-Headless Host Crates*
*Context gathered: 2026-08-22*
