---
title: "Compiled Podcast Wiki"
category: concepts
sources:
  - raw/notes/2026-05-09-knowledge-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [wiki, llm-wiki, compilation, synthesis]
aliases: [Podcast LLM Wiki, Generated Podcast Wiki]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Podcastr adapts llm-wiki by compiling episode transcripts into durable episode, show, person, concept, and cross-show pages."
---

# Compiled Podcast Wiki

The compiled podcast wiki adapts llm-wiki's core idea: synthesize once, query many times. Instead of re-answering every question from raw transcript chunks, Podcastr should compile reusable pages that accumulate knowledge over time.

## Page Types

- Episode pages: summary, chapters, topics, claims, guests, and notable timestamped spans.
- Show pages: recurring themes, hosts, favorite guests, release rhythm, and knowledge coverage.
- Person pages: speaker identity, appearances, topics, and notable claims.
- Concept pages: cross-episode synthesis for recurring subjects.
- Thread pages: the evolution of an argument across episodes or shows.
- Briefing pages: scripts and source anchors for generated audio summaries.

## Differences From llm-wiki

- Generation is automatic, triggered by episode ingestion and transcript readiness.
- Sources are episode spans, not only URLs or documents.
- Confidence reflects transcript quality, diarization quality, and synthesis faithfulness.
- The UI renders the wiki as rich podcast-native surfaces, not only Markdown.
- Exported Markdown remains useful for inspection, backup, or Obsidian.

## See Also

- [[knowledge-pipeline|Knowledge Pipeline]] ([Knowledge Pipeline](../topics/knowledge-pipeline.md)) - automatic compilation trigger path.
- [[retrieval-and-citation-model|Retrieval And Citation Model]] ([Retrieval And Citation Model](retrieval-and-citation-model.md)) - how compiled pages and raw chunks interact.
- [[data-model-notes|Data Model Notes]] ([Data Model Notes](../references/data-model-notes.md)) - storage implications.

## Sources

- [Knowledge source map](../../raw/notes/2026-05-09-knowledge-source-map.md)
