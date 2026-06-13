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
  - podcast-tui
supersedes: []
related_claims: []
source_lines:
  - 9556-9649
  - 9725-9734
captured_at: 2026-06-13T21:21:24Z
---

# Episode: CI workspace-build gate — full workspace must compile before merge

## Prior State

CI only ran cargo build/check for -p nmp-app-podcast (plus xcodebuild test). podcast-tui and other workspace members were never compiled in CI. A green PR that removed an FFI DTO used by podcast-tui auto-merged and broke cargo build --workspace on main.

## Trigger

During this session, a PR (#435) auto-merged on narrow-green CI, breaking the workspace because the TUI depended on a deleted type. The fleet auto-merges on green before review fixes are applied, and no CI gate caught the cross-member break.

## Decision

Add a Rust workspace-build gate job (cargo check --workspace --all-targets) on ubuntu-latest, with Cargo.toml-hash caching. This compiles all 8 workspace members (including podcast-tui) before any PR can merge. Merged as #440.

## Consequences

- Future FFI-DTO removals or cross-member API breaks are caught before reaching main
- Auto-merge fleet no longer merges code that breaks cargo build --workspace
- Must be added to main's branch-protection required checks to actually block (repo-admin setting, not CI-yaml)
- Cargo.lock is gitignored so --locked flag is wrong; gate uses --all-targets + Cargo.toml-hash cache instead

## Open Tail

- Branch-protection required-checks must be manually configured by repo admin for the gate to actually block auto-merge

## Evidence

- transcript lines 9556-9649
- transcript lines 9725-9734

