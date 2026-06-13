---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - ci-workspace-gate
  - cargo-check-workspace
  - auto-merge-safety
supersedes: []
related_claims: []
source_lines:
  - 9228-9230
  - 9362-9393
  - 9580-9589
  - 9630-9648
captured_at: 2026-06-13T19:46:29Z
---

# Episode: Add CI workspace-build gate to catch cross-crate FFI-DTO breaks

## Prior State

CI only compiled and linted -p nmp-app-podcast. podcast-tui and podcast-agent-core (workspace members depending on nmp-app-podcast by path) were never compiled in CI. Auto-merge on green meant a PR that removed an FFI DTO used by podcast-tui could land and break cargo build --workspace on main.

## Trigger

PR #435 auto-merged with a broken workspace build (podcast-tui still referenced deleted AgentNoteSummary) because CI's narrow scope reported green. The fleet auto-merges PRs the instant CI goes green, before review fixes can be applied.

## Decision

Add a cargo check --workspace --all-targets --locked gate on a Linux runner (disk-light, no Xcode/Gradle) to catch any workspace-member breakage before auto-merge.

## Consequences

- Prevents the exact regression class that broke main this session (FFI-DTO removal breaking a workspace consumer behind a green check)
- Linux runner with cargo cache keeps gate fast (~2-4 min)
- Also gates Cargo.lock drift via --locked flag

## Open Tail

- Android Gradle CI compile (compileDebugKotlin + unit tests) is a separate, disk-heavier item to sequence after this gate lands

## Evidence

- transcript lines 9228-9230
- transcript lines 9362-9393
- transcript lines 9580-9589
- transcript lines 9630-9648

