# Phase 1: Headless Host Crates - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-22
**Phase:** 1-Headless Host Crates
**Areas discussed:** Credential Handling, Observability, Tokio Runtime Consolidation, CI Scope

Mode: `--auto` — all areas auto-selected, each question auto-resolved to its recommended option without interactive prompts.

---

## Credential Handling

| Option | Description | Selected |
|--------|-------------|----------|
| Keep env vars | `pod0-cli` continues reading `POD0_OPENAI_API_KEY` etc. from environment variables | ✓ |
| Migrate to keyring | Route credentials through the existing `pod0-system-hosts` keyring integration used for Nostr keys | |

**User's choice:** [auto] Recommended default selected — env vars are the standard headless/CI-friendly pattern; a CI runner has no keyring session to unlock.
**Notes:** README already documents a mitigation ("credentials never returned by the CLI protocol"). Keyring migration deferred as a future, non-blocking improvement.

---

## Observability

| Option | Description | Selected |
|--------|-------------|----------|
| Add tracing now | Instrument `pod0-live-hosts`/`pod0-tts-host` with `tracing`/`tracing-subscriber` in this phase | ✓ |
| Defer | Leave observability gap for a later hardening pass | |

**User's choice:** [auto] Recommended default selected — closes the exact gap `.planning/codebase/CONCERNS.md` flagged, cheap to add alongside crates already being hardened here.
**Notes:** Subscriber installed only in `pod0-cli`; facade stays synchronous and silent.

---

## Tokio Runtime Consolidation

| Option | Description | Selected |
|--------|-------------|----------|
| Single shared Runtime | Constructed once at the `pod0-cli` entrypoint, threaded down as a `Handle` | ✓ |
| Per-adapter `Handle::current()` | Each host adapter lazily obtains a handle, assuming it always runs inside a runtime | |

**User's choice:** [auto] Recommended default selected — deterministic, matches PITFALLS.md's grep-audit guidance, avoids a panic if a component runs outside any runtime context.
**Notes:** Marked costly-to-reverse in CONTEXT.md — reworking after adapters are written against `Handle::current()` would touch every host crate's call sites.

---

## CI Scope

| Option | Description | Selected |
|--------|-------------|----------|
| In scope for Phase 1 | Confirm the existing `cargo deny` job covers the six new crates and new deps (`reqwest`, `tokio`) | ✓ |
| Separate follow-up | Treat as new CI infrastructure work outside this phase | |

**User's choice:** [auto] Recommended default selected — `cargo deny` already runs in the existing pipeline (`.planning/codebase/INTEGRATIONS.md`); this just confirms scope, not new CI work.
**Notes:** None.

---

## Claude's Discretion

- Exact tracing correlation-ID/span-naming scheme — follow existing `pod0-live-hosts` error-type idioms
- Tokio runtime flavor (multi-thread vs current-thread) — a planning-level decision based on actual crate concurrency needs

## Deferred Ideas

- Full keyring-backed credential storage for `pod0-cli` — future ADR candidate, not blocking this phase
- Voice-specific turn provenance tagging for Run History — already tracked as v2 in REQUIREMENTS.md, unrelated to this phase's scope
