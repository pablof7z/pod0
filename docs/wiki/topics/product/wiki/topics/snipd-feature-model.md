---
title: "Snipd Feature Model"
category: topics
sources:
  - raw/notes/2026-05-09-snipd-feature-research.md
  - ../../../../../spec/research/snipd-feature-model.md
created: 2026-05-09
updated: 2026-05-09
tags: [product, snipd, snips, entity-graph, chapters]
aliases: [Snipd Parity Model, Competitive Feature Model]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Snipd's learning features imply a span-first episode-processing model: transcripts, speaker IDs, chapters, highlights, snips, books, guest profiles, and exports are all derived from processed episodes."
---

# Snipd Feature Model

Snipd's visible product is not just "AI summaries." It is a processed-episode system where transcript spans become saved snips, chapters, guest pages, book pages, AI DJ routes, and exports.

## Core Pattern

The reusable pattern is:

1. Process the episode into timestamped transcript spans with speaker identity.
2. Run extraction passes for chapters, summaries, highlights, guests, books, and other entities.
3. Resolve extracted entities into canonical people, books, topics, and episodes.
4. Let ambient capture actions create durable artifacts over transcript spans.
5. Export or replay those artifacts as notes, quote cards, audio/video clips, or guided playback.

This maps cleanly to Podcastr's knowledge pipeline. The product should expose a state richer than "transcript ready": `publisher_metadata`, `transcript_ready`, `speaker_ready`, `chapters_ready`, `entities_ready`, `wiki_ready`, and `snips_ready`.

## Required Parity Surfaces

- **Headphone and CarPlay snipping**: ambient capture should create a span-grounded `Snip`, but control mapping must be user-configurable so skip-back remains available.
- **Auto-snipping**: a passive highlighter that proposes important spans while listening; user accepts, edits, exports, or deletes.
- **Auto-chapters**: publisher-first, AI fallback; feed skip-intro/outro, chapter skip from headphones, Episode Detail navigation, and AI DJ routes.
- **Mentioned books**: first-class `Book` entities with title, author, cover, description, mention context, show-level top-book pages, and timestamp citations.
- **Guests**: first-class `Person` pages with bio, portrait/source, appearances, follow state, similar people, and identity confidence.
- **AI DJ route**: a playable sequence of original-audio spans with generated bridge narration; distinct from the synthetic briefing player.
- **Exports**: snips and wiki entries should emit Markdown, quote images, and audio/video clips from the same source model.

## Data Model Pressure

The central object is the transcript span. Chapters, snips, quotes, book mentions, guest appearances, similar-guest evidence, and generated summaries all need stable pointers back to span IDs and timestamps.

Podcastr should avoid one-off feature storage. A saved snip, a mentioned-book context card, and a guest appearance card should all be different views over the same provenance model.

## Product Edge

Snipd proves that "save a moment from headphones" is the right behavior, but Podcastr can improve it with:

- provenance visible on every generated claim;
- local-library-first guest similarity instead of opaque global popularity;
- safer remote-command mapping;
- wiki-native exports rather than integrations as an afterthought;
- privacy modes that fall back to publisher transcripts or on-device transcription when cloud AI is not acceptable.

## See Also

- [[capability-map|Capability Map]] ([Capability Map](capability-map.md)) - where this fits in the product pillars.
- [[compiled-podcast-wiki|Compiled Podcast Wiki]] ([Compiled Podcast Wiki](../../../knowledge/wiki/concepts/compiled-podcast-wiki.md)) - entity pages generated from transcript evidence.
- [[learning-and-knowledge-exports|Learning And Knowledge Exports]] ([Learning And Knowledge Exports](../../../adjacent/wiki/concepts/learning-and-knowledge-exports.md)) - export and review loop.

## Sources

- [Snipd feature research](../../raw/notes/2026-05-09-snipd-feature-research.md)
- [Spec research note](../../../../../spec/research/snipd-feature-model.md)
