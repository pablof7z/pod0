---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - headless-e2e
  - ci-gate
  - signing-proof
supersedes:
  - 2026-06-14-4-e2e-proofs-must-be-ci-gated
related_claims: []
source_lines:
  - 10666-10686
  - 10869-10876
  - 10886-10894
captured_at: 2026-06-14T02:04:38Z
---

# Episode: Headless e2e suite must gate CI — signing proof was on-demand only

## Prior State

The headless e2e binary (including #446's per-podcast signing proof) ran only on-demand. .github/workflows/test.yml had 5 jobs but none invoked cargo run --bin headless. A regression in the kernel signing seam could reach main undetected. The 4 'failing' scenarios were assumed broken in CI sandboxes.

## Trigger

Cycle-14 planner investigated and found: (1) 3 scenarios already Skip gracefully on missing infra (ollama/relay/nak probes), (2) rss_subscribe uses loopback-only mock (CI runners allow loopback), (3) nipf4_publish is fully network-free via sign_tap. The real root cause of 'failing' scenarios was that the headless capability host stubbed nmp.http.async.capability, producing empty RSS placeholders.

## Decision

Add a headless-e2e CI job on ubuntu-latest (no relay/ollama/nak infra needed). Binary exit code is the gate; Skip = green by construction. The implementor also built a real async HTTP capability host (handle_http_async → reqwest thread → http_report back to kernel) so rss_subscribe/comments pass rather than stub out. A probe_loopback() guard prevents false FAIL in edge-case sandboxes.

## Consequences

- The signing proof (#446) and 6 other offline scenarios now automatically gate regressions
- Network-dependent scenarios Skip gracefully (not FAIL), keeping CI green
- The async HTTP capability host makes rss_subscribe and comments genuinely PASS on CI, not just skip
- Regressions in kernel signing, snapshot, or playback seams are now caught pre-merge

## Open Tail

- Whether to add a --require flag that fails CI if a named scenario Skips (preventing silent erosion of the offline core)

## Evidence

- transcript lines 10666-10686
- transcript lines 10869-10876
- transcript lines 10886-10894

