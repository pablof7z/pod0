---
title: RAG Service
slug: rag-service
topic: data-persistence
summary: RAGService raises a fatalError on VectorIndex initialization failure even when running in-memory
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-14
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:0f3f24f7-54de-49f8-b160-a92f735f6a00
  - session:f11c47b8-a7bd-47d3-9eb0-79dd02904d04
  - session:rollout-2026-05-09T14-56-23-019e0c98-8803-7ef0-b7a2-bf0b605a2360
  - session:rollout-2026-05-11T09-10-30-019e15a8-96ed-76a3-9539-607404bb9a31
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# RAG Service

## Initialization & Error Handling

RAGService raises a fatalError on VectorIndex initialization failure even when running in-memory. This acts as a build-misconfiguration guard, ensuring that improper setups are caught immediately rather than failing silently. <!-- [^0f3f2-62] -->

## Indexing & Discovery

Untranscribed episodes are indexed into the RAG corpus using their title and description, making them discoverable by find_similar_episodes and search_episodes. <!-- [^f11c4-4] -->

## Architecture & Retrieval

The RAG system uses OpenRouterEmbeddingsClient, VectorIndex, and RAGSearch for transcript ingestion and hybrid retrieval. RAG reranking is gated by `settings.rerankerEnabled`; when disabled, no OpenRouter rerank call is made even if an adapter requests reranking. The RAG search ownership is inverted from doctrine: iOS owns a full native VectorIndex/RAGService/SQLiteVec/EmbeddingsClient while the kernel's knowledge.rs search projection is an explicit substring stub, and a real podcast-knowledge crate exists unwired. However, this inverted ownership works for users today, and wiring podcast-knowledge into the kernel would be a multi-week embeddings-dimension migration. (Previously: the inversion and unwired crate were noted without acknowledging feasibility.)

<!-- citations: [^rollo-18] [^rollo-118] [^c1691-412] [^c1691-428] -->
## Error Handling

Without an OpenRouter key, transcript RAG errors are suppressed unless there are no other results, keeping local search usable. <!-- [^rollo-19] -->
