---
title: "Capability Map"
category: topics
sources:
  - raw/notes/2026-05-09-product-seed.md
  - raw/notes/2026-05-09-repo-spec-sources.md
  - raw/notes/2026-05-09-snipd-feature-research.md
created: 2026-05-09
updated: 2026-05-09
tags: [product, architecture, capabilities]
aliases: [Product Capability Map]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "The product decomposes into baseline playback, transcript ingestion, span-grounded snips, entity extraction, wiki compilation, retrieval, agent tools, voice, Nostr, and polished iOS UX."
---

# Capability Map

Podcastr has two layers that must mature together: the podcast-player floor and the agentic knowledge layer. The agent is only persuasive if the audio app underneath is already trustworthy.

## Baseline Audio App

The baseline includes subscription management, playback, queueing, downloads, search, transcripts, background audio, Now Playing, CarPlay, widgets, sync, accessibility, sharing, and privacy controls. These are category expectations, not differentiators.

## Knowledge Layer

The knowledge layer turns episodes into structured, searchable memory:

1. Discover publisher transcript.
2. Fall back to ElevenLabs Scribe or another transcription provider.
3. Normalize transcript segments, speakers, word timings, and source metadata.
4. Chunk and embed transcript spans.
5. Extract chapters, highlights, books, guests, topics, quotes, claims, and action items.
6. Resolve extracted entities into canonical `Book`, `Person`, `Topic`, `Show`, and `Episode` records.
7. Compile episode, speaker, book, concept, and show wiki pages.
8. Index wiki sections separately from transcript chunks.
9. Preserve citations back to episode timestamps.

## Snipd-Parity Layer

Snipd's public feature set raises the floor for podcast learning apps. Podcastr needs span-grounded snips, ambient headphone/CarPlay capture, auto-snipping, mentioned-book extraction, guest profiles with appearances and similar people, AI-generated chapters, and an AI DJ-style route through original-audio highlights.

These should not be separate bolt-ons. They are all views over the same processed-episode graph: transcript spans, speaker identities, chapter boundaries, entity mentions, and generated summaries.

## Agent Layer

The embedded agent should operate through handles and tools:

- `search_episodes`
- `query_transcripts`
- `query_wiki`
- `play_episode_at`
- `generate_briefing`
- `summarize_episode`
- `find_similar_episodes`
- `perplexity_search`
- `open_screen` or equivalent UI-routing tools

This keeps the system prompt compact while giving the agent precise access to the library.

## Voice And Briefing Layer

Voice mode supports live orders and interruptible briefings. Briefings are not just summaries: they are generated audio programs with anchors back to episodes and the wiki.

## Nostr Layer

Nostr provides remote communication with the user's agent. It is a command surface and needs explicit boundaries for actions that affect playback, messages, purchases, external research, or data sharing.

## Experience Layer

The experience target is an editorial, cinematic, Liquid Glass iOS app. The knowledge should appear as tappable timestamp chips, rich episode pages, cross-episode thread views, and agent answers that can move the app.

## See Also

- [[product-vision|Product Vision]] ([Product Vision](product-vision.md)) - the product promise behind these capabilities.
- [[snipd-feature-model|Snipd Feature Model]] ([Snipd Feature Model](snipd-feature-model.md)) - competitive model behind snips, books, guests, and AI chapters.
- [[agent-runtime-and-context|Agent Runtime And Context]] ([Agent Runtime And Context](../../../agent/wiki/topics/agent-runtime-and-context.md)) - how the agent should access these capabilities.
- [[experience-north-star|Experience North Star]] ([Experience North Star](../../../experience/wiki/topics/experience-north-star.md)) - UX principles for exposing them.

## Sources

- [Product seed](../../raw/notes/2026-05-09-product-seed.md)
- [Repo spec source map](../../raw/notes/2026-05-09-repo-spec-sources.md)
- [Snipd feature research](../../raw/notes/2026-05-09-snipd-feature-research.md)
