---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - ci-workspace-gate
  - podcast-tui
  - auto-merge-policy
supersedes:
  - 2026-06-13-2-ci-workspace-build-gate-prevent-ffi
related_claims: []
source_lines:
  - 9362-9393
  - 9580-9589
  - 9697-9734
captured_at: 2026-06-13T20:08:59Z
---

# Episode: CI must build the full Rust workspace — not just nmp-app-podcast

## Prior State

CI only compiled and tested -p nmp-app-podcast. The self-hosted test job and android-check job were both scoped to the single crate. podcast-tui and podcast-agent-core (workspace members depending on nmp-app-podcast by path) were never compiled in CI. Fleet auto-merges PRs the moment CI goes green.

## Trigger

PR #435 auto-merged with a green CI check (scoped to nmp-app-podcast) but broke cargo build --workspace because podcast-tui still referenced the deleted AgentNoteSummary. Main was workspace-broken until PR #437 repaired it. The same class of break could recur for any FFI-DTO removal.

## Decision

Add a cargo check --workspace --all-targets CI gate (PR #440, merged as 78976214). All 8 workspace members now compile on every PR. Repo-admin must also add it to branch-protection required checks to actually block auto-merge.

## Consequences

- Path-dependent consumer breaks (like podcast-tui referencing deleted symbols) are now caught by CI before auto-merge
- The narrow -p nmp-app-podcast scope that let a workspace-breaking PR through is closed
- Future FFI-DTO removals will fail CI if any consumer still references them
- Workspace check runs on ubuntu-latest (disk-light), ~2-4 min with cargo cache

## Open Tail

- Branch-protection required-checks list must be updated by repo admin for the gate to actually block auto-merge (not just report)

## Evidence

- transcript lines 9362-9393
- transcript lines 9580-9589
- transcript lines 9697-9734

