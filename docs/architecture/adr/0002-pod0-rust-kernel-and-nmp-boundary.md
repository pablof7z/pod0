# ADR-0002: Pod0 Rust kernel and NMP boundary

- Status: Accepted
- Date: 2026-07-18
- Decision owners: Pod0 application architecture
- Related issues: #57, #60, #63, #73

## Context

Master has no Rust workspace or active generic NMP integration. A previous
Swift NMP surface was removed after product narrowing. Pod0 still intends to
use Nostr for identity, coordination, publishing, and remote-agent flows, but
generic NMP must not become a home for podcast application concepts.

## Implementation status

The workspace and exact NMP pin landed after this decision. NMP remains
isolated in `pod0-nmp`; no application crate or iOS binary consumes it while
security issue #85 is open. The generated native facade is app-owned and no
Swift NMP surface has been restored.

## Decision

Create one Pod0-owned Rust workspace with cohesive crates or modules for:

- domain identities, schemas, and invariant-bearing models;
- application commands, actor/reducer, workflows, and projections;
- persistence and versioned migrations;
- platform capability contracts;
- a Pod0 NMP adapter and Pod0-specific event semantics;
- one app-owned UniFFI facade.

Generic NMP is a pinned dependency. It owns reusable Nostr protocol,
cryptography, relay, sync, routing, and signer primitives. Podcast, episode,
subscription, queue, transcript, highlight, clip, note, briefing, workflow,
and Pod0 agent nouns remain in Pod0 crates.

The dependency revision and Rust toolchain are locked. Upgrades are deliberate:
review release/upstream changes, run conformance and portability tests, update
the lockfile, and record any semantic migration.

## Boundary

Pod0 application code asks its NMP adapter for semantic operations. App-facing
commands never accept relay URLs, cipher choices, retry policy, or raw protocol
routing. NMP returns verified observations/provenance to the Pod0 application
layer; the application layer decides Pod0 product meaning.

Platform key custody is a capability boundary. Keychain/Keystore or an external
signer executes an authorized signing request. Rust owns the intent, scope,
permission decision, event semantics, and result validation.

## Failure behavior

- Unsupported or unverifiable private routing fails closed.
- Unknown recipient inbox/routing does not fall back to public relays.
- Protocol and capability failures become bounded diagnostic/action state.
- No raw secret, key material, relay credential, or private payload enters a
  normal UI projection or log.

## Migration

The Rust workspace is additive. No deleted Swift NMP layer is restored. Nostr
product flows enter only after the facade and earlier local vertical slices
prove persistence, recovery, binding, and single-writer behavior.

## Consequences

- NMP can evolve independently of Pod0 product releases.
- Pod0 can add product semantics without weakening a generic framework.
- Android consumes the same Pod0 facade rather than generic NMP directly.
- Nostr work is intentionally later than the first local listening slice.

## Rejected alternatives

- **Put Pod0 types in NMP:** violates generic framework ownership and couples
  releases.
- **Rebuild a Swift Nostr subsystem:** creates a platform-specific protocol
  owner and repeats removed work.
- **Track NMP main without a pin:** makes builds and semantics non-reproducible.
- **Expose relay/cipher choices through app commands:** moves routing and
  privacy policy into the shell.
