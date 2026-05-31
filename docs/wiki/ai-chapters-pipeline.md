---
title: AI Chapters Pipeline
slug: ai-chapters-pipeline
summary: AI chapters use a typed retry ladder (Ollama structured output with monotonicity + bounds validation) and persist to Rust
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-31
updated: 2026-05-31
verified: 2026-05-31
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
---

# AI Chapters Pipeline

## Chapter Generation Pipeline

AI chapters use a typed retry ladder (Ollama structured output with monotonicity + bounds validation) and persist to Rust. The `handle_index_episode` function clears stale chunks before upserting new ones to prevent stale knowledge accumulation. [^14943-100]

## See Also

