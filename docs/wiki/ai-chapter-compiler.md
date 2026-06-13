---
title: AI Chapter Compiler
slug: ai-chapter-compiler
topic: chapter-compilation
summary: AIChapterCompiler uses a dedicated `chapterCompilationModel` setting (default `openai/gpt-4o-mini`), separate from the wiki model setting
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-11
updated: 2026-06-13
verified: 2026-05-11
compiled-from: conversation
sources:
  - session:7f076ca6-6975-44ae-9848-d41832e499f0
  - session:9833dc25-72f9-4d4f-98d9-df476ead3e6d
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# AI Chapter Compiler

## Model Configuration

AIChapterCompiler uses a dedicated `chapterCompilationModel` setting (default `openai/gpt-4o-mini`), separate from the wiki model setting. AI chapter + ad-span generation is now D0 (kernel-owned): Rust port covers FULL/enrich-only modes, ad-validation rules (monotonic, non-overlapping, end>start, duration-capped), and stub chapters on Unavailability; 501 lines of Swift AIChapterCompiler deleted. The Rust FULL chapter path adds a retry ladder (Simple prompt on Parse failure, equal-length stubs on Unavailable) that the deleted Swift compiler never had — a deliberate improvement documented in the PR. The offline-FULL-mode chapter stub behavior is a deliberate product improvement: offline episodes now get generic `Chapter N` stubs where the deleted Swift compiler gave none. When AIChapterCompiler.swift was deleted, the `overlapsAd` extension was relocated to `Episode+AdOverlap.swift`, after the review caught that the Rust-only test pass had masked an orphaned Swift compile break.

<!-- citations: [^7f076-1] [^7f076-2] [^9833d-1] [^c1691-129] [^c1691-148] [^c1691-165] [^c1691-189] [^c1691-228] [^c1691-263] [^c1691-276] -->
