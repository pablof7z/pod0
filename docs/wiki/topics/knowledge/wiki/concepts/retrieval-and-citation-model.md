---
title: "Retrieval And Citation Model"
category: concepts
sources:
  - raw/notes/2026-05-09-knowledge-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [rag, retrieval, citations, embeddings]
aliases: [RAG Citation Model, Timestamp Citations]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "The agent should query compiled wiki pages first, fall back to transcript RAG, and return citations that can jump to episode timestamps."
---

# Retrieval And Citation Model

Retrieval should distinguish between compiled wiki knowledge and raw transcript evidence. They answer different questions and need different chunking strategies.

## Retrieval Layers

- Wiki retrieval: semantic chunks from compiled pages, useful for broad synthesis and repeated topics.
- Transcript retrieval: time-grounded chunks from raw transcript segments, useful for exact quotes, fuzzy recall, and timestamp jumps.
- Keyword retrieval: titles, show notes, speakers, chapter labels, and BM25 over chunk text.
- Reranking: optional for normal queries and skippable for low-latency voice mode.

The agent can combine results, but the indexes should remain distinct.

## Citation Contract

Every answer that references podcast content should expose:

- episode id
- show id
- start and end timestamp
- speaker when known
- transcript or wiki source
- confidence
- play action

The UI can render this as a timestamp chip, source card, or play button. The agent should not produce unsupported claims when the cited span is weak.

## Query Policy

Use wiki-first retrieval for synthesis. Use transcript retrieval when the user asks where something was said, asks for a quote, asks for a playable part, or asks fuzzy listening-history questions.

## See Also

- [[compiled-podcast-wiki|Compiled Podcast Wiki]] ([Compiled Podcast Wiki](compiled-podcast-wiki.md)) - why wiki retrieval exists.
- [[tool-surface|Tool Surface]] ([Tool Surface](../../../agent/wiki/concepts/tool-surface.md)) - agent tools that expose retrieval.
- [[core-surfaces|Core Surfaces]] ([Core Surfaces](../../../experience/wiki/concepts/core-surfaces.md)) - how citations appear in the UI.

## Sources

- [Knowledge source map](../../raw/notes/2026-05-09-knowledge-source-map.md)
