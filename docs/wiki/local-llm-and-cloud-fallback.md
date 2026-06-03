---
title: Local LLM and Cloud Fallback
slug: local-llm-and-cloud-fallback
summary: A local Gemma model is embedded in the app to handle lighter queries (search, summaries), while heavier queries fall back to a cloud provider.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-03
updated: 2026-06-03
verified: 2026-06-03
compiled-from: conversation
sources:
  - session:4dd36f3c-199e-4d1b-9f63-2f86c41e2f2a
---

# Local LLM and Cloud Fallback

## Local-Cloud Model Split

A local Gemma model is embedded in the app to handle lighter queries (search, summaries), while heavier queries fall back to a cloud provider. [^4dd36-2]


The local model is delivered as a post-install download (~2.6GB), not bundled in the IPA. [^4dd36-3]

## Local Model Selection

Gemma4-E2B (~2.58GB) is the practical local model choice for iPhone, featuring mobile-optimized MatFormer architecture rather than just quantization. [^4dd36-4]

## Local Inference Framework

LiteRT-LM is the target framework for local Gemma inference on iOS, replacing the deprecated MediaPipe LLM Inference. [^4dd36-5]

LiteRT-LM is integrated as an infrastructure bridge via FFI, where Swift exposes a single generateText function and Rust owns the business logic of what to ask and what to do with the answer. [^4dd36-6]
## See Also

