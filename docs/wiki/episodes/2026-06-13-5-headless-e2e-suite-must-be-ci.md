---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - headless-ci-gate
  - e2e-regression-enforcement
supersedes: []
related_claims: []
source_lines:
  - 10673-10685
captured_at: 2026-06-13T23:44:31Z
---

# Episode: Headless e2e suite must be CI-enforced or it rots — gate to be added

## Prior State

The headless e2e binary (including the #446 signing proof) was run only on-demand — nothing in .github/workflows/test.yml invoked it, so regressions in the kernel signing seam or other offline scenarios could reach main undetected.

## Trigger

Review of #444/#446 discovered the gap; planner verified that 3 of 4 'failing' scenarios already Skip gracefully (probe_tcp on ollama/relay/nak), rss_subscribe uses loopback-only mock, and the harness already treats Skip as exit-0.

## Decision

Add a headless-e2e CI job to test.yml on ubuntu-latest: cargo run --bin headless --features headless. Skip = green by construction. The 4 fully-offline scenarios (rss_subscribe, key_persistence, identity, nipf4_publish) will genuinely PASS and gate regressions; network-dependent scenarios self-Skip.

## Consequences

- E2e proofs like #446's signing seam now automatically gate regressions
- The signing-proof test that was previously dead weight in CI now has enforcement teeth
- Future offline scenarios are automatically covered

## Open Tail

- Optional follow-up: --require flag to fail if a named scenario Skips, preventing silent erosion of the offline core
- 4 network-dependent scenarios (rss_subscribe, inbox_triage, comments, social) timeout/fail in sandbox but Skip or pass on normal runners

## Evidence

- transcript lines 10673-10685

