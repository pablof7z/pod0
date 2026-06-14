---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - ci-gates
  - branch-protection
  - main-integrity
supersedes:
  - 2026-06-13-2-ci-must-build-the-full-rust
related_claims: []
source_lines:
  - 10005-10007
captured_at: 2026-06-13T22:49:46Z
---

# Episode: CI protection: Rust workspace build and Android Kotlin must gate main merges

## Prior State

Main branch broke twice this session from fleet auto-merges because Rust workspace compilation and Android Kotlin unit tests were not required checks — they reported but did not block auto-merge.

## Trigger

Main broke twice from unguarded auto-merges that passed limited CI but had compilation/test failures in unguarded configurations.

## Decision

Add Rust workspace build gate and Android Kotlin compile + unit tests to main's branch-protection required checks so they block the fleet's auto-merge rather than merely reporting.

## Consequences

- Unguarded Kotlin/Gradle or Rust workspace compilation failures now block auto-merge
- Both CI gaps that caused main breakage are closed (PRs #440 and #441)
- Future fleet auto-merges will be blocked until all required checks pass

## Open Tail

*(none)*

## Evidence

- transcript lines 10005-10007

