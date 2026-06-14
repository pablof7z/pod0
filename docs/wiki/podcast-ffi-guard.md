---
title: Podcast FFI Guard
slug: podcast-ffi-guard
topic: agent-system
summary: Every extern "C" entry across the entire `src/ffi/` module is wrapped in a shared `ffi_guard` helper that catches panics and returns a degrade sentinel
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

# Podcast FFI Guard

## FFI Panic Guard

Every extern "C" entry across the entire `src/ffi/` module is wrapped in a shared `ffi_guard` helper that catches panics and returns a degrade sentinel. A panic across `extern "C"` on this toolchain is a deterministic abort, not UB (since rustc 1.81+), so the guard must cover the whole module rather than a named list. `panic="abort"` is explicitly rejected because it would nullify both this guard and `nmp_core`'s `catch_unwind` around actor ticks. The `ffi_guard` fallback uses a lazy `impl FnOnce() -> T` instead of an eager `T`, and logs the caught-panic site via `log::error!` on the `Err` path before returning the fallback sentinel. (Previously: the ffi_guard fallback was eager T, which was changed to lazy impl FnOnce() -> T after an Opus review caught a systematic memory leak: the old design allocated CString fallbacks via into_raw() before body() even ran, leaking on every success path across ~18 string-returning extern entries.)

<!-- citations: [^c1691-47] [^c1691-124] -->
## Known Build Issues

The podcast Rust module's android.rs previously had invalid Rust numeric-literal suffixes (0jint, -1jint, 0jlong) from the ffi_guard fan-out that broke cross-compilation. This is now fixed with `0 as jint`/`-1 as jint`/`0 as jlong` casts (PR #401), and a CI android-check job compiles the cfg-gated JNI surface on every PR via `cargo check --target aarch64-linux-android`. (Previously: the invalid suffixes were invisible to CI because no workflow targeted aarch64-linux-android.) The CI workspace-build gate (`cargo check --workspace --all-targets`) compiles all 8 workspace members including podcast-tui, catching FFI-DTO removals that the `-p nmp-app-podcast`-scoped lint missed. FFI-DTO removal PRs must grep the entire workspace (not just apps/nmp-app-podcast) because podcast-tui and podcast-agent-core are path-dependent workspace members that consume the same projection structs.

<!-- citations: [^c1691-48] [^c1691-83] [^c1691-97] [^c1691-335] [^c1691-392] -->
## Serialization Guards

Required non-Option float fields in PodcastUpdate projections must use a `finite_or_zero` serialization guard to prevent NaN/Inf from serializing as null and dropping the entire Swift frame. Only `ChapterSummary.start_secs` and `TranscriptEntry.start_secs` are genuinely reachable for NaN propagation from untrusted input; the other 5 of 7 sites are false positives because JSON cannot encode NaN for action-dispatch/provider seams and `download.progress` plus `knowledge.relevance` are already clamped at the producer. <!-- [^c1691-125] -->

## Priority Order

The corrected priority order is: (1) ffi_guard across all `src/ffi/*` (reject panic=abort), (2) `parse_duration` finite-guard + serde guards on ChapterSummary/TranscriptEntry.start_secs, (3) kill the subscribed-library deep clone, (4) extract a BackgroundLlmJob runner making the signal mandatory so D8 compliance is structural not disciplinary, (5) Swift XCTest golden-fixture decode, (6) LLM consolidation + dead-code deletion. <!-- [^c1691-126] -->

## Kernel Time Authority

The connectedAt timestamp on provider credentials must be stamped by the kernel on receipt of the set-credential action, not by either shell's wall clock (D9: kernel owns time). <!-- [^c1691-379] -->
