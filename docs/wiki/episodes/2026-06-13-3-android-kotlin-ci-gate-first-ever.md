---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - ci-android-kotlin
  - android-gradle
  - compiledebugkotlin
supersedes: []
related_claims: []
source_lines:
  - 9593-9603
  - 9906-9944
captured_at: 2026-06-13T21:21:24Z
---

# Episode: Android Kotlin CI gate — first-ever Kotlin compilation in CI

## Prior State

Android had 64 .kt files and a full Gradle project, but zero Kotlin was compiled in CI. No CI job invoked gradlew at all. PR #430 was literally titled 'repair DomainFrameWireTest compile' — a Kotlin compile break that reached main silently.

## Trigger

PR #439 (Android profile resolution) required manual Gradle verification because no CI gate existed for Kotlin. The planner identified this as a structural blind spot, confirmed by the manual compileDebugKotlin + testDebugUnitTest run that was needed before merge.

## Decision

Add android-kotlin-check CI job running full JDK/SDK/NDK/cargo-ndk + Gradle stack: ./gradlew :app:compileDebugKotlin :app:compileDebugUnitTestKotlin :app:testDebugUnitTest. All tool versions sourced from repo files (JDK 17, AGP 8.5.2, Kotlin 1.9.24, Gradle 8.7, NDK r27c). Merged as #441, validated on its own CI run (Kotlin gate went green in ~12 min).

## Consequences

- Android Kotlin changes now compile before merge for the first time ever
- The preBuild→cargoNdk Gradle task dependency means any Kotlin CI job must also set up the full Rust Android cross-compile stack (cargo-ndk + both ABIs)
- Android-Gradle is disk-heavy on CI runners (SDK + Gradle daemon + dependency cache); sequenced after the lighter workspace gate (#440)
- Must also be added to branch-protection required checks to block auto-merge

## Open Tail

- cargo-ndk is unpinned in CI (no version in repo configs) — should be pinned if a specific version is established later

## Evidence

- transcript lines 9593-9603
- transcript lines 9906-9944

