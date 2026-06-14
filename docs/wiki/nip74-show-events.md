---
title: NIP-74 Show Events
slug: nip74-show-events
topic: nostr-protocol
summary: NIP-F4 show events do not emit a d tag
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-26
updated: 2026-06-14
verified: 2026-05-26
compiled-from: conversation
sources:
  - session:378a594b-f095-461d-a035-4d3afca30d5e
  - session:rollout-2026-05-17T17-57-58-019e3671-a863-7ab1-a96d-6ceb8b541971
  - session:rollout-2026-05-25T12-53-39-019e5e8d-ec64-74f1-a1b1-91055dcab442
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# NIP-74 Show Events

## NIP-F4 Show Events

NIP-F4 show events do not emit a d tag. They use a "description" tag instead of "summary". The publisher wire format for shows must use kind 10154 with description (not summary) and no d tag.

The NIP74Show type has no d_tag field, and its coordinate() method returns "10154:<pubkey>" with no d-tag component. Its summary field is renamed to description.

The podcast_to_show_tags function parameter changed from agent_pubkey to podcast_pubkey, making the signer the per-podcast key.

NIP-09 deletion for owned podcasts emits a single kind:5 event with both `k:10154` (show) and `k:54` (episode) tags, tombstoning the full per-podcast footprint. The `delete_owned` NIP-09 deletion uses a kind-targeted `k`-tag form without a specific `e`-tag event id because the kernel signs and does not return the event id at dispatch time. (Previously: deletion switched from e-tag to k-tag targeting kind:10154 only.) The delete_owned docstring incorrectly states it targets kind:10154/54 but the code only emits [k, 10154]; the docstring should be corrected.

The show parser no longer requires a d tag and does not return MissingTag("d"). It reads the "description" tag (with content fallback) instead of "summary".

ShowReference is still valid for parsing legacy a tags and remains unchanged.

The show_d_tag_round_trips_lowercase test is replaced by show_coordinate_is_stable_per_pubkey.

NIP-F4 discovery uses the shared NDK instance as the sole relay-management mechanism (Nostr PodastDiscoveryService is the only component that directly calls addRelay() and connect() on it), with no secondary NDK instantiations or separate one-shot websocket connections. (Previously: NIP-F4 discovery must use the app-owned NDK relay pool and its existing connections, not a separate one-shot websocket connection, superseded — see nostr-rust-ffi.)

A publishAuthorClaim function must be added as kind 10064.

Tool/schema/UI/docs copy must be updated from NIP-74/naddr to NIP-F4/event id or podcast identity.

AgentToolsPodcastTests.testPublishEpisodeSuccessReturnsNaddr and MockOwnedPodcasts must be updated to expect event id instead of naddr.

<!-- citations: [^378a5-2] [^378a5-3] [^378a5-4] [^378a5-5] [^378a5-6] [^378a5-7] [^rollo-164] [^rollo-182] [^c1691-299] [^c1691-314] [^c1691-326] [^c1691-341] -->
