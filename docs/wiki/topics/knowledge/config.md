---
title: "Podcastr Knowledge Wiki"
description: "Transcript ingestion, wiki compilation, embeddings, RAG, citations, and data model decisions."
created: 2026-05-09
freshness_threshold: 30
---

# Wiki Configuration

## Scope

This topic wiki captures how Podcastr turns podcast episodes into durable, queryable, timestamp-grounded knowledge.

## Conventions

- Prefer wiki-first answers with transcript-RAG fallback.
- Every synthesized claim should eventually be traceable to an episode span.
- Treat embeddings and vector indexes as derived data.
- Keep transcript, wiki, and briefing artifacts separate even when they share retrieval infrastructure.
