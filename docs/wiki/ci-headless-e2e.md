---
title: CI Headless E2E
slug: ci-headless-e2e
topic: project-setup
summary: "CI includes three verification gates: Rust workspace build (cargo check --workspace --all-targets), Android Kotlin compile + unit tests (Gradle), and headless e"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-13
updated: 2026-06-14
verified: 2026-06-13
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# CI Headless E2E

## Headless E2E CI Gate

CI includes three verification gates: Rust workspace build (cargo check --workspace --all-targets), Android Kotlin compile + unit tests (Gradle), and headless e2e kernel proofs — closing the three regression classes that broke main this session. The Rust gate catches FFI-DTO removals that break podcast-tui or podcast-agent-core; when removing an FFI-DTO field, the entire workspace including podcast-tui must be grepped, not just apps/nmp-app-podcast, because the TUI binds the same PodcastUpdate and projection structs by path dependency. The Android gate runs compileDebugKotlin + testDebugUnitTest; previously, zero Kotlin was compiled in CI and narrow-scoped gates let both the podcast-tui break and the Kotlin compile error reach main undetected. The headless e2e binary runs in CI (ubuntu-latest, headless-e2e job) with Skip treated as exit-0 by construction and Fail as the only failure condition; network-free scenarios (nipf4_publish, rss_subscribe, key_persistence, identity_import, discover_nostr, comments, agent_notes) genuinely PASS offline, so CI can run on ubuntu-latest without relay/ollama/nak infra. The nipf4_publish scenario originally asserted last_published_at as a pass condition, but that was false confidence because the handler stamps unconditionally before publish dispatch and register_podcast_signer_in_kernel has no failure return; the strengthened scenario instead asserts the actual signed event's pubkey and sig via the D13 sign-for-return path (nmp_app_sign_event_for_return) as the observable that proves the kernel signed with the per-podcast key, with a mutation-check proof (commenting out register_podcast_signer_in_kernel makes the test FAIL). The headless harness must provide a real async HTTP capability (nmp.http.async.capability) by spawning a real reqwest thread and feeding results back via nmp_app_podcast_http_report; the old stub produced empty RSS placeholders and was the root cause of rss_subscribe/comments scenario failures.

<!-- citations: [^c1691-323] [^c1691-350] [^c1691-371] [^c1691-382] [^c1691-400] [^c1691-433] [^c1691-446] -->
