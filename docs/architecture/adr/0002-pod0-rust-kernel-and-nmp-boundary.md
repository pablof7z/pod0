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

The workspace and exact NMP pin landed after this decision. The iOS app now
owns one upstream `NMPEngine`; Pod0 does not wrap its identity, signing,
routing, transport, query, or receipt product surface in Rust or Swift.

## Decision

Create one Pod0-owned Rust workspace with cohesive crates or modules for:

- domain identities, schemas, and invariant-bearing models;
- application commands, actor/reducer, workflows, and projections;
- persistence and versioned migrations;
- platform capability contracts;
- Pod0-specific publication intent and audit semantics at the app boundary;
- one app-owned UniFFI facade.

Upstream NMP is a pinned Swift-package dependency. It owns Nostr identity,
cryptography, relay, sync, routing, and signer primitives. Podcast, episode,
subscription, queue, transcript, highlight, clip, note, briefing, workflow,
and Pod0 agent nouns remain in Pod0 crates.

The dependency revision and Rust toolchain are locked. Upgrades are deliberate:
review release/upstream changes, run conformance and portability tests, update
the lockfile, and record any semantic migration.

## Boundary

Pod0 composes a bounded product publication draft, then hands the generic write
directly to the app-owned `NMPEngine`. App-facing commands never accept relay
URLs, cipher choices, retry policy, or raw protocol routing. Pod0 may retain a
receipt identifier and bounded product audit facts; NMP remains authoritative
for identity, signing, routing, transport, queries, and receipt state.

NMP's account store owns platform key custody. Pod0 never receives a private
key or exposes a signing request across its facade.

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
