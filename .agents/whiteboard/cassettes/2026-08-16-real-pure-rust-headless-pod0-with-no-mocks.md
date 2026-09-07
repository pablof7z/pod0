---
type: whiteboard-decision-cassette
id: 2026-08-16-real-pure-rust-headless-pod0-with-no-mocks
date: 2026-08-16
status: active
supersedes:
  - 2026-08-16-rust-core-first-agent-interface
source_whiteboard: pod0/2026-08-16-agent-repl-access-exploration
---

# Real pure-Rust headless Pod0 with no mocks

## Decision

The target is a real, pure-Rust, headless Pod0 application controlled through
an agent-friendly REPL or machine protocol. Its feature paths must execute real
effects. Nothing may be mocked: an unavailable capability must fail explicitly
as unsupported until its real implementation exists.

The intended feature scope includes the real Pod0 business core, persistent
settings, live AI agents and provider calls, audio playback, clip rendering and
export, feeds, downloads, and transcription.

## Why This Is Material

This changes the work from a facade test harness into another real application
shell. It also prevents future implementations from declaring feature coverage
through canned model responses, simulated playback, fixture network results,
or synthetic host observations.

## Verbatim User Statements

> how easily could we make some kind of repl UI for pod0 such that any agent could very easily use pod0 without the swift part, without having to click things around such that it can test any part of the app for real: a player, the agent, the clipping, the AI features, etc, etc

Source: user message at 2026-08-16T22:55:25.316+03:00

> before you move forward; I want to understand what youre saying - you are not talking about building anything that's "fake" or "synthetic" right? it's a real app that will run as a repl with full features, real AI agent running, real way of configuring any setting, etc, etc?

Source: user clarification at 2026-08-16T23:01:18.416+03:00

> Pure Rust headless app with real portable capabilities (Recommended)

Source: user selection in response to the implementation-boundary question

> nothing should be mocked. Absolutely nothing.

Source: subsequent user clarification

## Explicit User Agreement

- Proposition: The implementation boundary is a pure-Rust headless app with
  real portable capabilities.
  Evidence: The user selected “Pure Rust headless app with real portable
  capabilities (Recommended)” over controlling the existing Swift runtime.
- Proposition: No feature or effect may be mocked.
  Evidence: “nothing should be mocked. Absolutely nothing.”

## Inferred Acceptance

- Missing capabilities should remain visibly unavailable rather than receive
  synthetic success behavior.
  Basis: This is the safe operational consequence of the explicit no-mocks
  requirement; the user did not separately choose the error representation.

## Agent Assumptions

- The REPL should expose both an interactive interface and a stable
  machine-readable mode.
  Depends on: interface design.
  If wrong: one mode can be omitted without changing the real-effect
  requirement.
  Validation: unverified.
- Portable implementations need behavioral parity with Pod0 product semantics,
  but do not prove the shipped Apple adapters.
  Depends on: how test claims are described.
  If wrong: Apple-adapter validation requires a separate native test surface.
  Validation: supported by the current ownership boundary, not explicitly
  chosen by the user.

## Evidence And Constraints

- Pod0 already has a typed Rust application facade and Rust-owned durable
  product policy for substantial feature areas.
- The current Swift shell still supplies real playback, clip export,
  credentials, networking, downloads, providers, and platform lifecycle.
- These native capabilities therefore require real Rust replacements before
  the headless application can claim those features.
- Product business policy must remain singular; portable hosts execute effects
  but must not duplicate Rust-owned decisions.
- Secrets must not enter normal projections or logs.

## Unresolved Tensions / Risks

  the definition of complete feature parity remain open.
- A portable implementation validates the headless app's real behavior, not
  AVFoundation, Keychain, background URLSession, or other shipped Apple
  adapter behavior.
- Full feature coverage is substantially larger than the previously estimated
  facade-only shell.

## Rejected / Deferred Alternatives

- Deterministic or fixture-backed host effects.
  Disposition: rejected.
  Why: They violate the explicit no-mocks requirement.
  Authority: user.
- Headless control of the existing Swift app runtime.
  Disposition: rejected as the target architecture.
  Why: The user selected the pure-Rust option.
  Authority: user.

## Supersession Conditions

- The user permits a mocked capability or changes the pure-Rust boundary.
- A required capability cannot be implemented portably and the user chooses a
  native bridge instead.
