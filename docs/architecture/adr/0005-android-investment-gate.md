# ADR-0005: Android investment gate

- Status: Accepted
- Date: 2026-07-18
- Decision owners: Pod0 product and application architecture
- Related issues: #61, #62, #63, #72, #77

## Context

Android should consume proven Pod0 behavior, not become a second experiment or
a manual translation of Swift policy. Kotlin bindings and Android-compatible
Rust compilation are necessary architecture checks but do not prove product
value or justify substantial Android application work.

## Decision

Substantial Android application work requires an explicit go/hold/stop ADR from
milestone M5. Before results are inspected, Pod0 defines metric semantics,
privacy constraints, minimum evidence/confidence, and budgets for:

- repeat/retained product use after activation;
- successful listening, resume, interruption, and route recovery;
- transcript/search/recall usefulness and correct playable citations;
- highlights, notes, or clips as evidence of retained knowledge value;
- agent engagement and grounded completion;
- crash-free use, unrecoverable data loss, and migration safety;
- launch, projection, retrieval, and playback performance;
- proportion of priority durable behavior owned outside Swift;
- unclassified Swift policy and Apple-specific assumptions in shared APIs.

The gate report records evidence quality and returns:

- **Go:** product and architecture thresholds pass; M6 may start.
- **Hold:** evidence is incomplete or a named gap has a bounded remediation
  path; M6 remains closed.
- **Stop:** evidence contradicts the investment thesis; do not start M6.

## Pre-gate Android work allowed

- Generate and compile Kotlin bindings from the same facade as Swift.
- Build Rust for Android targets in CI.
- Keep core APIs free of Apple types.
- Maintain a minimal non-product compile/smoke harness.
- Run shared deterministic behavior fixtures.

No Compose product shell, Media3 integration, feature-parity implementation,
or Android release work begins under this allowance.

## Android ownership after a go

Kotlin/Compose owns native UI, navigation, transient presentation, Media3,
WorkManager, notifications/media session, Keystore, permissions, and Android
system integration. The Rust kernel remains the only owner of durable product
facts and decisions. Parity means equivalent behavior for prioritized outcomes,
not visual imitation of iOS.

## Failure behavior

An underpowered, missing, privacy-invalid, or contradictory dataset cannot be
rounded up to a go. The result is hold or stop. A green compile job cannot
override missing product evidence.

## Consequences

- iOS validation remains the immediate product priority.
- Portability regressions are found early without starting a second app.
- Android sequencing is evidence-driven rather than calendar-driven.
- A hold returns concrete issues to M1-M4 and keeps M6 gated.

## Rejected alternatives

- **Start Android when bindings compile:** confuses technical feasibility with
  product validation.
- **Build both apps in parallel now:** doubles surface area before behavior is
  stable.
- **Wait to consider Android portability until after iOS:** permits Apple
  assumptions and Swift-only schemas to harden.
- **Use a discretionary unrecorded decision:** invites post-hoc success criteria.
