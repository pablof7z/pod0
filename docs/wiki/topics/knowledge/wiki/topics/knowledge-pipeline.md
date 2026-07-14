---
title: "Knowledge Pipeline"
category: topics
sources:
  - raw/notes/2026-05-09-knowledge-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [knowledge, pipeline, transcripts, embeddings, wiki]
aliases: [Episode To Knowledge Pipeline]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Podcastr turns new episodes into answerable knowledge through transcript discovery, transcription fallback, chunking, embeddings, wiki compilation, and timestamp citation."
---

# Knowledge Pipeline

The knowledge pipeline starts when a subscribed feed publishes or updates an episode. The output is not just a transcript; it is a set of queryable, cited knowledge artifacts that the agent and UI can use.

## Flow

1. Feed refresh discovers a new episode.
2. The app checks for a publisher-provided transcript.
3. If no useful transcript exists, the episode enters transcription fallback.
4. The transcript normalizer emits a single internal segment model with speaker, time, confidence, source, and optional word timings.
5. The chunker creates time-grounded transcript chunks.
6. OpenRouter embeddings index transcript chunks.
7. The wiki compiler updates episode, show, person, and concept pages.
8. Wiki sections are embedded separately from transcript chunks.
9. Agent tools query wiki first and transcript chunks when needed.
10. UI citations jump back to exact episode spans.

## Design Implication

The app should show readiness honestly. A fresh episode can be playable before it is transcript-ready, transcript-ready before it is wiki-ready, and wiki-ready before cross-show synthesis has been refreshed.

## See Also

- [[transcript-source-ladder|Transcript Source Ladder]] ([Transcript Source Ladder](../concepts/transcript-source-ladder.md)) - source priority for transcripts.
- [[compiled-podcast-wiki|Compiled Podcast Wiki]] ([Compiled Podcast Wiki](../concepts/compiled-podcast-wiki.md)) - what the compilation step creates.
- [[retrieval-and-citation-model|Retrieval And Citation Model]] ([Retrieval And Citation Model](../concepts/retrieval-and-citation-model.md)) - how answers stay grounded.
- [[agent-runtime-and-context|Agent Runtime And Context]] ([Agent Runtime And Context](../../../agent/wiki/topics/agent-runtime-and-context.md)) - how the agent accesses the pipeline.

## Sources

- [Knowledge source map](../../raw/notes/2026-05-09-knowledge-source-map.md)
