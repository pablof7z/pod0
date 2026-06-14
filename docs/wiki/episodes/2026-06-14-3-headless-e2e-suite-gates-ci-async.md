---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - headless-e2e-ci-gate
  - capability-host-async-http
supersedes:
  - 2026-06-14-3-headless-e2e-suite-must-gate-ci
related_claims: []
source_lines:
  - 10673-10685
  - 10825-10894
captured_at: 2026-06-14T02:13:47Z
---

# Episode: Headless e2e suite gates CI — async HTTP capability host fixes the 'failing' scenarios

## Prior State

Headless e2e suite was on-demand only with no CI enforcement; the signing proof (#446) gated nothing; 4 scenarios appeared to fail in sandboxed environments; the capability host stubbed nmp.http.async.capability causing RSS subscribes to produce empty placeholders

## Trigger

The signing proof from #446 was not gating regressions; investigation revealed the 'failing' scenarios were either Skip-graceful (3/4) or loopback-only (rss_subscribe), and the real root cause was the stubbed HTTP capability host

## Decision

Add headless-e2e CI job on ubuntu-latest (Skip = green by construction); implement a real async HTTP capability host (thread → reqwest → http_report back to kernel) replacing the stub; add probe_loopback() guard for sandboxed environments

## Consequences

- nipf4_publish signing proof and offline scenarios now gate regressions automatically in CI
- rss_subscribe/comments/nipf4_publish actually PASS (not skip) because the async HTTP host feeds real RSS results back to the kernel
- Skip-on-missing-infra pattern validated — network-dependent scenarios gracefully Skip on standard CI runners
- Test-infra only change — zero production code touched

## Open Tail

- Optional --require flag to fail if a named scenario Skips, preventing silent erosion of the offline core

## Evidence

- transcript lines 10673-10685
- transcript lines 10825-10894

