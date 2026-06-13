---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - ci-workspace-gate
  - podcast-tui-consumer
  - ffi-dto-retirement
supersedes:
  - 2026-06-13-3-add-ci-workspace-build-gate-to
related_claims: []
source_lines:
  - 9228-9231
  - 9363-9382
  - 9245-9247
  - 9578-9589
  - 9630-9634
  - 9694-9719
captured_at: 2026-06-13T19:50:41Z
---

# Episode: CI workspace-build gate: prevent FFI-DTO removals from breaking workspace behind a green check

## Prior State

CI only compiled and linted -p nmp-app-podcast (plus Xcode/Android builds scoped to that crate). Other workspace members like apps/podcast-tui (which depends on nmp-app-podcast by path) were never compiled in CI. The fleet auto-merges PRs the instant CI goes green.

## Trigger

PR #435 (retire flat agent_notes projection) auto-merged with a green CI check, but apps/podcast-tui still imported AgentNoteSummary and PodcastUpdate.agent_notes — breaking cargo build --workspace on main. The orphan grep in the PR was scoped only to apps/nmp-app-podcast and missed the third Rust consumer. CI's -p nmp-app-podcast-scoped lint couldn't see the break.

## Decision

Add a cargo check --workspace --all-targets CI gate (PR #440) that compiles every workspace member, not just nmp-app-podcast. The gate uses Cargo.toml-hash caching (not --locked, since Cargo.lock is gitignored). Also recorded as a durable lesson: FFI-DTO removals must grep the entire workspace including podcast-tui, because it binds PodcastUpdate and projection structs by path dependency.

## Consequences

- Future FFI-DTO field/type removals that break podcast-tui (or any other workspace member) will fail CI before auto-merge
- The gate must be added to GitHub branch-protection required checks (admin setting) to actually block auto-merge — otherwise it only reports green/red without blocking
- PR #437 (repair podcast-tui migration onto nostr_conversations) was needed as a follow-up because #435 auto-merged broken — this class of breakage is now structurally prevented

## Open Tail

- Branch-protection required-checks list needs admin update to include the new workspace-check gate
- Android Kotlin compile (compileDebugKotlin) remains unCI'd — a separate gap identified but not yet addressed (planned as #3)

## Evidence

- transcript lines 9228-9231
- transcript lines 9363-9382
- transcript lines 9245-9247
- transcript lines 9578-9589
- transcript lines 9630-9634
- transcript lines 9694-9719

