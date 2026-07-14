---
title: "Social And Creator Network"
category: concepts
sources:
  - raw/notes/2026-05-09-online-adjacent-research.md
created: 2026-05-09
updated: 2026-05-09
tags: [social, creators, nostr, activitypub, analytics]
aliases: [Creator Network, Social Podcast Layer]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Podcastr can use open social roots, creator recommendations, person tags, and public analytics to make discovery feel creator-aligned."
---

# Social And Creator Network

The product can become richer without building a closed social network. The open podcast ecosystem already has primitives for comments, recommendations, credits, and public stats.

## Product Ideas

- **Official episode discussion**: use socialInteract roots to show and post to the canonical discussion thread when supported.
- **Nostr bridge**: map Podcastr's existing Nostr agent communication to user-facing episode discussion carefully, without mixing private commands with public comments.
- **Creator recommendations**: render podrolls as "recommended by this show," separate from algorithmic discovery.
- **People-driven discovery**: follow a guest, host, producer, or contributor across shows.
- **Open stats context**: use OP3-style public analytics as a trust and discovery signal where available.
 
This intentionally excludes creator monetization tooling from the agent surface.

## Safety Boundary

Community and remote-control concepts must remain separate. A public episode comment is not an agent command. A Nostr DM command is not a public discussion reply.

## See Also

- [[nostr-command-safety|Nostr Command Safety]] ([Nostr Command Safety](../../../agent/wiki/concepts/nostr-command-safety.md)) - remote command boundaries.
- [[podcasting-2-rich-metadata|Podcasting 2 Rich Metadata]] ([Podcasting 2 Rich Metadata](podcasting-2-rich-metadata.md)) - source tags behind the network.
- [[product-vision|Product Vision]] ([Product Vision](../../../product/wiki/topics/product-vision.md)) - how social features support the core promise.

## Sources

- [Online adjacent research](../../raw/notes/2026-05-09-online-adjacent-research.md)
