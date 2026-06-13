---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - ci-android-kotlin
  - android-compile-gate
supersedes: []
related_claims: []
source_lines:
  - 9593-9603
  - 9906-9943
captured_at: 2026-06-13T20:08:59Z
---

# Episode: Android Kotlin must be CI-compiled — zero prior coverage

## Prior State

No CI job ever compiled Android Kotlin. The 64 .kt files in android/Podcast/ were completely unvalidated by CI. PR #430's title ('repair DomainFrameWireTest compile') is itself evidence of an unguarded Kotlin compile break reaching main. PR #439 required manual local Gradle verification before merge.

## Trigger

The same session that caught the Rust workspace gap also revealed that Android Kotlin has never been CI-gated. The preBuild→cargoNdk Gradle task dependency means any Kotlin compile also triggers a full Rust cross-compile, but neither was validated automatically.

## Decision

Add android-kotlin-check CI job (PR #441) running ./gradlew :app:compileDebugKotlin :app:compileDebugUnitTestKotlin :app:testDebugUnitTest with the full Android+NDK+Rust+cargo-ndk stack. All tool versions sourced from repo files (JDK 17, AGP 8.5.2, Kotlin 1.9.24, Gradle 8.7, NDK r27c).

## Consequences

- Android Kotlin compile breaks can no longer reach main silently
- The preBuild→cargoNdk dependency means the job also validates Rust cross-compilation for Android
- Job is disk-heavy (~8-12 Gi for Gradle + SDK + NDK + cargo-ndk) and slow (~15-25 min cold)
- cargo-ndk is unpinned (no version in repo config) — potential future flakiness source

## Open Tail

- PR #441's own CI run is the live proof — polling to completion
- Branch-protection required-checks must include 'Android Kotlin compile + unit tests' to block auto-merge
- If setup-android's sdkmanager is flaky on ubuntu-latest, may need SDK caching

## Evidence

- transcript lines 9593-9603
- transcript lines 9906-9943

