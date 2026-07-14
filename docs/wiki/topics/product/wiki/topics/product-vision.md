---
title: "Product Vision"
category: topics
sources:
  - raw/notes/2026-05-09-product-seed.md
  - raw/notes/2026-05-09-repo-spec-sources.md
created: 2026-05-09
updated: 2026-05-09
tags: [product, vision, agent, podcast-player]
aliases: [Podcastr Vision, Talk To Your Podcasts]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Podcastr is a podcast player where the user can converse with their whole podcast library, including episodes they have not listened to yet."
---

# Product Vision

Podcastr is a podcast player built around a knowledge-grounded embedded agent. The core promise is not only "play podcasts better"; it is "talk to all of my podcasts as if they were one continuous, searchable, interruptible conversation."

The product earns that promise by combining normal podcast-player competence with a durable knowledge layer. Episodes become timestamped transcript data, transcript data becomes compiled wiki pages, and those pages become the agent's working memory.

## Product Promise

The user can ask natural questions across their library:

- "Play the part where they talked about keto."
- "What was that podcast last week about stamps?"
- "Make me a TLDR of this week's podcasts."
- "What do my podcasts say about Ozempic across episodes?"

The answer should be playable, cited, and actionable. If the answer references an episode, the UI should make the timestamp jump obvious. If the answer becomes a briefing, the briefing should be audio-first and interruptible.

## Non-Negotiables

- Podcast playback must feel first-class before the agent layer matters.
- Transcripts must preserve timestamp and speaker context.
- Generated wiki claims need timestamp-backed citations.
- The agent should use tools to inspect the library instead of carrying the whole library in the system prompt.
- Voice interaction is a primary surface, not an accessory.
- Nostr communication is a real command channel and needs safety boundaries.
- UX must feel like a polished iOS audio app, not an AI demo wrapped in tabs.

## Boundaries

The copied prompt called for many parallel agents. This wiki does not preserve that as product scope. The durable requirement is the output those agents were meant to produce: clear UX principles, product shape, implementation map, and technical research.

## See Also

- [[capability-map|Capability Map]] ([Capability Map](capability-map.md)) - how the promise decomposes into product pillars.
- [[launch-floor|Launch Floor]] ([Launch Floor](../references/launch-floor.md)) - baseline podcast features needed for credibility.
- [[knowledge-pipeline|Knowledge Pipeline]] ([Knowledge Pipeline](../../../knowledge/wiki/topics/knowledge-pipeline.md)) - how episodes become answerable knowledge.

## Sources

- [Product seed](../../raw/notes/2026-05-09-product-seed.md)
- [Repo spec source map](../../raw/notes/2026-05-09-repo-spec-sources.md)
