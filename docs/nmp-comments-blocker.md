# Episode-comment reads: pinned NMP capability audit

Audited against the exact Pod0 pin
`317b7caaf5a83da1e6899efcc5aeb90a85b808c3` for Pod0 issue #10.
This audit covers verified reads only; publishing is a separate milestone.

## Available primitives

The pinned Swift SDK provides:

- generic `NMPEngine.observe(_:window:)` with
  `Window.expandable(initial:max:)`;
- generic `NMPFilter`/`NMPDemand`, `NMPQuery`, and raw `RowBatch` values;
- scoped acquisition, source, and shortfall facts on each generic batch.

These primitives do not establish NIP-22/NIP-73 semantics. A valid Nostr
signature on a raw row is not proof that its comment root, parent, or external
target is well formed or matches the selected podcast episode.

## Missing typed comment boundary

The pinned Swift package exports none of the typed APIs required for an M2
cutover: no NIP-73 podcast-episode target, NIP-22 comment decoder/validator,
typed root observation, or typed root-versus-reply relationship. In particular,
the speculative Pod0 names
`PodcastEpisodeCommentTarget`, `EpisodeCommentBatch`,
and `observeEpisodeComments` do not exist.

Using the generic filter API would require Pod0 to construct the kind 1111
root query and own `I/K/i/k` validation and relationship classification. That
is the raw protocol fallback forbidden by the ownership boundary.

Pod0 therefore remains fail-closed, with an explicit paused state and no
composer or retry action. The removed legacy comment service must not return.
The missing capability is tracked by `pablof7z/nmp#572`.
The exact pinned-surface evidence was also reported upstream in
`pablof7z/nmp#572` on 2026-07-16.

## Exit evidence for bug #572

Before Pod0 enables comments, the pinned Swift API must compile and prove:

1. typed `.podcastEpisode(guid:)` root observation with an authoritative
   `Window.expandable(initial: 200, max: 200)` snapshot;
2. typed validation that rejects malformed or mismatched NIP-22/NIP-73 tags;
3. typed root/parent facts so top-level comments can be presented without
   dropping replies from the root-thread observation;
4. ordinary scoped source/acquisition/shortfall evidence preserved alongside
   the semantic snapshot;
5. cancellation that withdraws only the bounded observation demand.
