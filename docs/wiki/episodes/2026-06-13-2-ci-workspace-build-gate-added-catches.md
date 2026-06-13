---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - ci-workspace-gate
  - cargo-check-workspace
  - podcast-tui
supersedes:
  - 2026-06-13-2-ci-workspace-build-gate-full-workspace
related_claims: []
source_lines:
  - 9697-9734
captured_at: 2026-06-13T21:48:25Z
---

# Episode: CI workspace build gate added — catches path-dependent consumer breaks

## Prior State

CI only ran checks scoped to -p nmp-app-podcast, missing path-dependent consumers like podcast-tui. A PR that removed an FFI DTO broke podcast-tui's compilation, but the narrow CI scope reported green, and the fleet auto-merged the PR — breaking main.

## Trigger

Main broke from an auto-merged PR that removed an FFI DTO used by podcast-tui; the narrow CI scope (-p nmp-app-podcast only) didn't catch it. The same class of break (removing AgentNoteSummary / PodcastUpdate.agent_notes via #437) also broke podcast-tui.

## Decision

Added a 'Rust workspace build gate' CI job running cargo check --workspace --all-targets, compiling all 8 workspace members including podcast-tui (PR #440, merged as 78976214). Also discovered that --locked is wrong because Cargo.lock is gitignored; used --all-targets with Cargo.toml-hash cache instead.

## Consequences

- All 8 workspace members now compiled on every PR; FFI-DTO removals that break consumers are caught before merge
- The gate must be added to main's branch-protection required checks to actually block auto-merge (a repo-admin setting not yet applied)
- Future cargo check --workspace failures won't silently reach main via auto-merge

## Open Tail

- Repo-admin must add 'Rust workspace build gate' to main's branch-protection required checks — otherwise it reports but doesn't block the fleet's auto-merge

## Evidence

- transcript lines 9697-9734

