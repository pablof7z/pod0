---
type: whiteboard-decision-cassette
id: 2026-08-16-rust-core-first-agent-interface
date: 2026-08-16
status: active
supersedes: []
source_whiteboard: pod0/2026-08-16-agent-repl-access-exploration
---

# Rust-core-first agent interface

## Decision

Pod0's first agent-accessible, non-UI interface will exercise the real Rust
application core before adding live portable hosts. Deterministic host effects
are acceptable in this first phase when they are identified as deterministic.
Live portable execution and control of the shipped Apple-native adapters are
deferred.

## Why This Is Material

This establishes the fidelity boundary for the first headless Pod0 runtime. It
prevents an initial implementation from either rebuilding every native
capability before proving the agent interface or presenting simulated effects
as equivalent to the shipped Apple adapters.

## Verbatim User Statements

> how easily could we make some kind of repl UI for pod0 such that any agent could very easily use pod0 without the swift part, without having to click things around such that it can test any part of the app for real: a player, the agent, the clipping, the AI features, etc, etc

Source: user message at 2026-08-16T22:55:25.316+03:00

> Real Rust core first, then add live portable hosts (Recommended)

Source: user selection in response to the v1 fidelity question

## Explicit User Agreement

- Proposition: Use the real Rust core first, then add live portable hosts.
  Evidence: The user selected “Real Rust core first, then add live portable
  hosts (Recommended)” from the presented v1 fidelity levels.

## Inferred Acceptance

- Deterministic host effects may stand in for live effects during the first
  phase if they are labeled honestly.
  Basis: The selected option was presented as the Rust facade/database level
  with deterministic host effects, but the selected label did not repeat this
  qualification.
- Apple-native fidelity is not required in the first phase.
  Basis: The user selected the Rust-core-first option instead of the separate
  running-iOS/macOS fidelity option.

## Agent Assumptions

- The first runtime should use an isolated store rather than the installed
  app's production data.
  Depends on: safe persistence defaults and avoiding concurrent writers.
  If wrong: the runtime needs an explicit attachment or import design.
  Validation: unverified.
- A machine-readable protocol should be primary and a human REPL secondary.
  Depends on: the interface shape.
  If wrong: v1 may prioritize interactive terminal ergonomics.
  Validation: inferred from the request that any agent use it easily, but not
  explicitly selected.

## Evidence And Constraints

- Pod0's public Rust facade already exposes command dispatch, bounded
  projections, leased host requests, and correlated host observations.
- The BDD suite already drives this public loop against real Rust storage.
- Durable product policy for migrated features is Rust-owned; actual playback,
  clip export, credentials, and several provider/platform effects remain in
  Swift hosts.
- The interface must not duplicate business policy, create a second durable
  writer, or describe deterministic effects as live execution.

## Unresolved Tensions / Risks

- The exact v1 command protocol, supported command catalog, operating systems,
  store bootstrap, and deterministic host set remain undecided.
- A Rust implementation of audio or provider hosts can test equivalent
  behavior but cannot validate the shipped AVFoundation, Keychain, background
  session, or NMP adapters.

## Rejected / Deferred Alternatives

- Live portable hosts from day one.
  Disposition: deferred.
  Why: The user selected Rust-core-first phasing.
  Authority: user.
- Control the running iOS/macOS app for maximum fidelity.
  Disposition: deferred.
  Why: The user selected Rust-core-first phasing.
  Authority: user.

## Supersession Conditions

- v1 is required to validate an Apple-native capability rather than Rust-owned
  product behavior.
- The facade cannot support a useful agent workflow without new native
  orchestration semantics.
- The user changes the desired fidelity or production-data boundary.
