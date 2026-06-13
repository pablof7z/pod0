---
title: CI Android Check
slug: ci-android-check
topic: project-setup
summary: The Android jint fix changed invalid Rust numeric literal suffixes (e.g., `0jint`, `-1jint`) to cast syntax (`0 as jint`, `-1 as jint`) and added an `android-ch
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-12
updated: 2026-06-13
verified: 2026-06-12
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# CI Android Check

## Android Check CI Job

The Android jint fix changed invalid Rust numeric literal suffixes (e.g., `0jint`, `-1jint`) to cast syntax (`0 as jint`, `-1 as jint`) and added an `android-check` CI job that runs `cargo check --target aarch64-linux-android` to prevent future invisible Android breakage. However, Android Kotlin is never compiled in CI (no Gradle invocation anywhere in .github/); this allowed a DomainFrameWireTest compile break to reach main and required manual Haiku verification.

The Android per-domain frame consumption uses `@SerialName` for snake_case field mapping and `ignoreUnknownKeys = true` on the JSON decoder, so Rust-only fields are safely dropped.

On Android, a push frame with no domains returns `null` from `decodeDomainFrames` and never touches state; a frame whose domains are all stale yields `anyAccepted=false` and also never touches state, fully removing the empty-clobber bug.

<!-- citations: [^c1691-249] [^c1691-250] [^c1691-177] [^c1691-191] [^c1691-219] [^c1691-248] [^c1691-296] -->
