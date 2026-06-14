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
updated: 2026-06-14
verified: 2026-06-12
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# CI Android Check

## Android Check CI Job

The Android jint fix changed invalid Rust numeric literal suffixes (e.g., `0jint`, `-1jint`) to cast syntax (`0 as jint`, `-1 as jint`) and added an `android-check` CI job that runs `cargo check --workspace --all-targets` to prevent future invisible Android breakage. Previously, it ran only `-p nmp-app-podcast`, which missed FFI-DTO removals that broke podcast-tui. Android Kotlin compilation and unit tests must run in CI (`compileDebugKotlin` + `testDebugUnitTest`) because the existing `android-check` job only ran `cargo check` on the Rust kernel, leaving Kotlin breaks undetected. DomainFrameWireTest.kt had a pre-existing compile error at line 428 (`agent!!.activeCount` does not exist on `AgentSnapshot`) that was undetected because Android unit tests were never gated in CI. On Android, a push frame with no domains returns `null` from `decodeDomainFrames` and never touches state; a frame whose domains are all stale yields `anyAccepted=false` and also never touches state, fully removing the empty-clobber bug. The Android per-domain frame consumption uses `@SerialName` for snake_case field mapping and `ignoreUnknownKeys = true` on the JSON decoder; `@SerialName` annotations are required because kotlinx-serialization has no auto-convert from snake_case like iOS's `convertFromSnakeCase`, so Rust-only fields are safely dropped.

<!-- citations: [^c1691-249] [^c1691-250] [^c1691-177] [^c1691-191] [^c1691-219] [^c1691-248] [^c1691-296] [^c1691-305] [^c1691-322] [^c1691-336] [^c1691-349] [^c1691-370] [^c1691-399] [^c1691-432] -->
