---
title: CI TestFlight Workflow
slug: ci-testflight-workflow
topic: project-setup
summary: The TestFlight workflow must be gated by tests or use workflow_run
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-11
updated: 2026-06-13
verified: 2026-05-11
compiled-from: conversation
sources:
  - session:rollout-2026-05-11T09-10-31-019e15a8-991d-7890-957e-f45fb0ff5a7c
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# CI TestFlight Workflow

## TestFlight Workflow

The TestFlight workflow must be gated by tests or use workflow_run. The deploy job depends on a test job. PRs that delete or move Swift files must run an actual xcodebuild compile (not just cargo test), because Rust-green can mask orphaned Swift references (as occurred with overlapsAd in AIChapterCompiler deletion). The CI iOS-sim destination fix uses runtime UDID discovery (prefer '* ci' simulator, fallback any available iPhone, exit 70 on none) instead of hardcoded 'iPhone 17' name, preventing future Xcode/simulator version breakage.

Headless scenarios are not run in CI (no workflow invokes cargo run --bin headless --features headless); the nipf4_publish e2e guard is on-demand only unless wired into a CI job. <!-- [^c1691-306] -->

<!-- citations: [^rollo-130] [^c1691-130] [^c1691-151] [^c1691-152] [^c1691-192] -->
## CI Test Command

The CI test command is shared in ci_scripts/run_tests.sh for use by both local and CI workflows. A CI job running cargo check --workspace --all-targets is needed because no current CI gate compiles the full workspace; podcast-tui and podcast-agent-core are never compiled in CI, allowing FFI-DTO removal PRs to break main behind a green check.

<!-- citations: [^rollo-131] [^c1691-131] [^c1691-297] -->
## App Store Connect Key Cleanup

App Store Connect key cleanup must remove persisted key material, not just the keychain. The cleanup script removes temporary App Store Connect .p8 key material, not just the keychain. <!-- [^rollo-132] -->

## App Runtime Code Restrictions

No app runtime Swift code outside of config/plists may be touched. <!-- [^rollo-133] -->
