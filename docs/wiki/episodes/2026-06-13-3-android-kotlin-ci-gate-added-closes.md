---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - ci-android-kotlin
  - android-gradle
  - compile-debug-kotlin
supersedes:
  - 2026-06-13-3-android-kotlin-ci-gate-first-ever
related_claims: []
source_lines:
  - 9886-9992
captured_at: 2026-06-13T21:48:25Z
---

# Episode: Android Kotlin CI gate added — closes the unguarded-Kotlin gap

## Prior State

No CI gate existed for Android Kotlin compilation; 64 .kt files were completely unvalidated. The existing android-check job only ran cargo check on the Rust lib, leaving Kotlin breaks invisible to CI. PR #430 was literally a 'repair Kotlin compile' PR, and #439 required manual Gradle verification before merge.

## Trigger

Two concrete Kotlin breaks reached main without CI detection: #430 (repair compile) and #439 (needed manual Gradle verify). The preBuild→cargoNdk Gradle task dependency means any Kotlin compile also triggers a full Rust cross-compile, so the gap couldn't be partially closed.

## Decision

Added 'android-kotlin-check' CI job with full JDK 17/SDK 34/NDK r27c/cargo-ndk + Gradle 8.7 pipeline running :app:compileDebugKotlin :app:compileDebugUnitTestKotlin :app:testDebugUnitTest (PR #441, merged as c289af54). All tool versions sourced from actual repo files rather than hardcoded.

## Consequences

- Android Kotlin now has CI coverage for the first time; JNI external-fun↔Rust export mismatches and kotlinx-serializer wiring breaks are caught before merge
- The gate validated itself on its own CI run (~12 min, Kotlin compiled + unit tests passed green)
- Repo-admin must also add this gate to branch-protection required checks to block auto-merge

## Open Tail

- cargo-ndk is unpinned (no version in repo config); if a specific version is later pinned, CI should match it
- Gradle SDK setup may be slow/flaky on ubuntu-latest; consider caching the Android SDK separately

## Evidence

- transcript lines 9886-9992

