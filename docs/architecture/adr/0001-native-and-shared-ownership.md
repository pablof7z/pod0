# ADR-0001: Native and shared ownership

- Status: Accepted
- Date: 2026-07-18
- Decision owners: Pod0 application architecture
- Related issues: #55, #63, #64

## Context

Pod0 is currently a native Swift application. Swift code contains both
legitimate Apple-platform behavior and durable product decisions that Android
would otherwise need to reproduce. iOS must remain fast to iterate without
turning the iOS proof phase into a second migration project.

## Decision

Pod0 uses four explicit ownership classifications.

### Shared Rust now

The Pod0 Rust kernel progressively owns stable, durable, cross-platform product
facts and decisions: identities and schemas, subscriptions and feed policy,
queue/resume/completion policy, workflow desired state, transcript and evidence
normalization, durable user artifacts, agent validation/commits, and
Pod0-specific Nostr semantics.

Each migrated fact has one Rust writer. Native code may retain a bounded view
projection but not a second authoritative cache or policy implementation.

### Native by design

Swift owns SwiftUI rendering, native navigation and transitions, accessibility,
animations, transient presentation state, AVFoundation, audio sessions and
routes, media controls, CarPlay, BGTask and URLSession entry points,
notifications, Keychain and biometric prompts, widgets, file/share pickers,
and other Apple integrations.

Native capability adapters execute typed requests and report raw typed
observations. They do not decide retry, fallback, recoverability, queue order,
completion, routing, privacy, or durable next state.

### Temporary Swift behind a migration-safe boundary

Unsettled product behavior may remain in Swift only when all of the following
are true:

- it is isolated behind a typed protocol or application adapter;
- its persisted format and behavior have characterization tests;
- the ownership inventory links a mandatory migration/deletion issue;
- it is not a second writer for a migrated domain;
- the replacement removes obsolete ownership as part of the same vertical
  slice.

### Planning, research, or decision record

Spikes and decisions own no production fact. An undecided owner must have a
time-bounded question, evidence output, recommendation, and blocking issue.

## Data and action boundary

- UI sends semantic intents to the current application owner.
- Rust commands become ordered actor input once a domain is migrated.
- Rust emits bounded, revisioned, screen-shaped projections.
- Native capabilities return nondeterministic inputs as explicit observations.
- High-frequency animation and raw media timing stay native; durable sampled
  playback observations cross at a bounded cadence.
- Secrets use explicit secure capability channels and never ordinary snapshots
  or logs.

## Failure behavior

Durable errors and cancellation are semantic state owned by the current domain
writer. Native presentation localizes and renders that state. A platform
capability reports raw failure; it does not decide whether to retry or recover.

## Migration

Domains move by user-visible vertical slice: model, commands, effects,
persistence, bindings, migration, integration, tests, cutover, and deletion.
Shadow reads may compare results before cutover. Long-lived dual writes are
forbidden.

## Consequences

- Swift is not debt merely because it is Swift.
- Shared APIs remain free of UIKit, AVFoundation, URLSession, and Android types.
- Some current Swift owners remain temporary until their issue cuts them over.
- UI iteration stays native and does not wait for unrelated Rust domains.

## Rejected alternatives

- **Keep all behavior in Swift until Android:** creates a predictable second
  implementation and late data migration.
- **Move all Swift code to Rust first:** blocks iOS product proof and wrongly
  moves platform-native behavior.
- **Share presentation/UI:** degrades native quality and couples platforms.
- **Allow permanent dual writers:** makes recovery and migration correctness
  unprovable.
