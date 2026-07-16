# Episode comments: pinned NMP capability audit

Audited against the exact Pod0 pin
`317b7caaf5a83da1e6899efcc5aeb90a85b808c3`.

## Available primitives

The pinned Swift SDK provides:

- generic `NMPEngine.observe(_:window:)` with
  `Window.expandable(initial:max:)`;
- generic durable `NMPEngine.publish(_:)` and receipt status streams;
- `NMPEngine.reattachReceipt(id:)` with attached, missing, and unreadable
  outcomes.

## Missing typed comment boundary

The pinned Swift package exports none of the typed APIs required for an M2
cutover: no NIP-73 podcast-episode target, NIP-22 comment decoder/validator,
typed root observation, top-level comment composer, or typed pending comment
projection. In particular, the speculative Pod0 names
`PodcastEpisodeCommentTarget`, `EpisodeCommentBatch`,
`observeEpisodeComments`, and `episodeCommentIntent` do not exist.

Using the generic filter and write-intent APIs would require Pod0 to construct
kind 1111 filters, `I/K/i/k` tags, validation, and routing itself. That is the
raw protocol fallback forbidden by the ownership boundary.

Pod0 therefore remains fail-closed, with an explicit paused state and no
composer or retry action. The removed legacy comment service must not return.
The missing capability is tracked by `pablof7z/nmp#572`.

## Exit evidence for bug #572

Before Pod0 enables comments, the pinned Swift API must compile and prove:

1. typed `.podcastEpisode(guid:)` root observation with an authoritative
   `Window.expandable(initial: 200, max: 200)` snapshot;
2. typed validation that rejects malformed or mismatched NIP-22/NIP-73 tags;
3. a durable top-level comment intent with module-owned author-outbox routing;
4. canonical pending comment correlation plus receipt reattachment after
   process restart;
5. cancellation that withdraws only the observation demand.
