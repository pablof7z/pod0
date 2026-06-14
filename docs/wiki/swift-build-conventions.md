---
title: Swift Build Conventions
slug: swift-build-conventions
topic: project-setup
summary: PRs that delete Swift files must run `xcodebuild build-for-testing` (the test target globs `AppTests/**`) to catch orphaned references that app-only builds miss
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

# Swift Build Conventions

## Deleting Swift Files

PRs that delete Swift files must run `xcodebuild build-for-testing` (the test target globs `AppTests/**`) to catch orphaned references that app-only builds miss. The kernel AI chapters + ad-spans port (PR #413) required relocating the `overlapsAd` extension from the deleted `AIChapterCompiler.swift` into a new surviving file to fix a Swift compile failure that Rust-only tests masked. `KernelSigner`, `NostrSigner` protocol, and `NostrEventDraft` are dead code (zero callers after #418) and were deleted; `NostrSignerError` is retained because `KernelBridge.swift` and `SignedEventsRegistryTests.swift` still use it.

<!-- citations: [^c1691-216] [^c1691-173] [^c1691-188] [^c1691-200] [^c1691-215] [^c1691-293] [^c1691-304] [^c1691-347] -->
## Pre-existing Compile Errors Outside CI

The `DomainFrameWireTest.kt:428` `agent!!.activeCount` reference is a compile error because `AgentSnapshot` has no `activeCount` field — a pre-existing bug from PR #423 that was undetected because Android unit tests are not CI-gated. <!-- [^c1691-294] -->

## Shared Root Git Operations

No agents (not just reviewers) may perform working-tree git operations in the shared root, to prevent data loss like the identity-WIP discard incident. <!-- [^c1691-394] -->

## Kernel-Stamped Timestamps

The connected_at timestamp on provider credentials must be kernel-stamped (chrono::Utc::now().timestamp()) on receipt of the set-credential action, removing host-side Date()/System.currentTimeMillis() from both iOS and Android payloads, per D9 (kernel owns time). <!-- [^c1691-395] -->
