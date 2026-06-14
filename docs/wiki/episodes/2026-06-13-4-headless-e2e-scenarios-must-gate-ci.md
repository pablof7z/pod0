---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - headless-ci-gate
  - e2e-regression-gate
supersedes:
  - 2026-06-13-5-headless-e2e-suite-must-be-ci
related_claims: []
source_lines:
  - 10667-10717
captured_at: 2026-06-14T00:00:10Z
---

# Episode: Headless e2e scenarios must gate CI — currently on-demand only

## Prior State

The headless e2e binary (including #446's per-podcast signing proof) runs only on-demand. .github/workflows/test.yml has no headless invocation. E2e guards protect nothing automatically — a regression in the kernel signing seam would reach main undetected.

## Trigger

Investigation confirmed headless isn't in any CI workflow; 3 of 4 'failing' scenarios already Skip gracefully on missing infra; nipf4_publish is fully network-free; the harness treats Skip as exit-0. The sandbox failures are an artifact, not broken scenarios.

## Decision

Add a headless-e2e CI job to test.yml running on ubuntu-latest: cargo run --bin headless --features headless. Skip = green by construction. Network-free scenarios (nipf4_publish, key_persistence, identity) genuinely PASS offline; network-dependent scenarios Skip via probe_tcp.

## Consequences

- E2e guards (including signing proof) will auto-gate regressions on every PR
- rss_subscribe needs loopback TCP (CI runners allow it)
- Optional --require flag to fail if named scenarios Skip (prevent silent erosion of offline core)

## Open Tail

- Implementor dispatched but not yet merged; CI run itself is the live proof

## Evidence

- transcript lines 10667-10717

