---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - headless-e2e-ci
  - capability-host
  - e2e-doctrine
supersedes:
  - 2026-06-14-5-headless-e2e-proofs-must-be-ci
related_claims: []
source_lines:
  - 10673-10683
  - 10869-10875
  - 10888-10894
captured_at: 2026-06-14T01:50:50Z
---

# Episode: E2e proofs must be CI-gated — headless suite wired in with real async HTTP capability

## Prior State

The headless e2e binary (including #446's per-podcast signing proof) was on-demand only — nothing ran it in CI. The headless capability host stubbed nmp.http.async.capability, so RSS subscribes produced empty placeholders and 4 scenarios failed/timeout. The harness already treated Skip as exit-0 but the gate was never enforced.

## Trigger

The #444/#446 signing proof reached main undetected by any CI check — exactly the failure class workspace-check and kotlin-check were created to close. Further, investigation showed the 4 'failing' scenarios were actually 3 graceful Skips (probe_tcp on ollama/relay) plus 1 loopback-blocked scenario, and the root cause was the stubbed HTTP capability making RSS fetches produce empty results.

## Decision

Added headless-e2e CI job (ubuntu-latest, Skip=green by construction). Implemented a real async HTTP capability host (handle_http_async decodes HttpCommand, spawns reqwest in a std thread, calls http_report back to the kernel's FeedFetchCoordinator). Added probe_loopback() guard in rss_subscribe for sandboxed environments. Doctrine: e2e proofs must be CI-enforced or they rot; Skip-on-missing-infra is the right pattern (already established by relay_smoke.rs).

## Consequences

- 7 scenarios now PASS on CI (rss_subscribe, key_persistence, identity_import, nipf4_publish, discover_nostr, comments, agent_notes), network-dependent ones SKIP gracefully
- nipf4_publish's signing proof + offline scenarios now gate regressions automatically
- The signing proof is no longer theoretical — CI enforces it on every PR
- Headless scenarios exercise real RSS fetching, not empty placeholders
- 3 CI integrity holes closed across this session: workspace-check (#440), Android Kotlin (#441), headless e2e (#449)

## Open Tail

- Optional hardening: --require flag that fails if a named scenario Skips, preventing silent erosion of the offline core
- 4 network-dependent scenarios (rss_subscribe with real relay, inbox_triage, comments, social) need relay/ollama infra or graceful Skip in CI
- Branch protection should add all three gates as required checks

## Evidence

- transcript lines 10673-10683
- transcript lines 10869-10875
- transcript lines 10888-10894

