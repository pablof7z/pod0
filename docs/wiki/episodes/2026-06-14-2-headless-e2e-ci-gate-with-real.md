---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - headless-e2e-ci
  - capability-host
  - rss-subscribe
supersedes:
  - 2026-06-14-3-headless-e2e-suite-gates-ci-async
related_claims: []
source_lines:
  - 10868-10876
  - 10894-10899
captured_at: 2026-06-14T03:48:31Z
---

# Episode: Headless e2e CI gate with real async HTTP capability host

## Prior State

Headless e2e scenarios were 'failing' because the capability host stubbed nmp.http.async.capability — every RSS subscribe produced an optimistic placeholder with no episodes; there was no CI gate for the signing proof or offline scenarios

## Trigger

PR #449 implementor discovered the root cause: the stubbed HTTP capability host meant rss_subscribe, comments, and the episode branch of nipf4_publish all failed; the headless suite had no CI enforcement

## Decision

Implement a real async HTTP capability host (thread → reqwest → http_report back to the kernel's FeedFetchCoordinator) plus a headless-e2e CI job (ubuntu-latest, timeout 20min, Skip-as-green); probe_loopback() guard for sandboxed environments

## Consequences

- nipf4_publish signing proof + offline scenarios now gate regressions automatically in CI
- rss_subscribe, comments, and nipf4_publish pass (not skip) on ubuntu-latest because RSS fetches actually complete
- Test-infra only change — zero production code touched
- The three CI integrity gates (workspace build, Android Kotlin, headless e2e) now collectively close the verification holes that let broken code reach main

## Open Tail

- All three gates (Rust workspace, Android Kotlin, headless e2e) should be added to main's branch-protection required checks to block the fleet's auto-merge

## Evidence

- transcript lines 10868-10876
- transcript lines 10894-10899

