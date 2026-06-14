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
  - async-http-capability
supersedes:
  - 2026-06-13-4-headless-e2e-scenarios-must-gate-ci
related_claims: []
source_lines:
  - 10664-10900
captured_at: 2026-06-14T00:26:33Z
---

# Episode: Headless e2e proofs must be CI-enforced; stubbed HTTP capability was root cause of 'failing' scenarios

## Prior State

Headless e2e suite wasn't in CI — signing proof (#446) and offline scenarios gated nothing automatically. The 4 'failing' scenarios appeared broken in sandbox; the headless capability host STUBBED nmp.http.async.capability, causing RSS subscribes to produce empty placeholders

## Trigger

After #446 merged, its e2e proof still gated nothing in CI. Investigation of the 4 'failing' scenarios revealed: 3 already Skip gracefully (probe ollama/relay/nak), and 1 (rss_subscribe) uses a loopback mock that only fails in sandboxed environments — but the deeper root cause was the stubbed async HTTP capability

## Decision

Added headless-e2e CI job on ubuntu-latest (exit code gate, Skip = green). Implemented real async HTTP capability host (thread → reqwest → nmp_app_podcast_http_report back to kernel's FeedFetchCoordinator) replacing the stub. Added probe_loopback() guard in rss_subscribe for edge-case sandbox configs

## Consequences

- nipf4_publish signing proof + offline scenarios now gate regressions automatically in CI
- rss_subscribe and comments actually exercise RSS fetch in CI (not just stubs)
- Skip-on-missing-infra pattern validated: network-bound scenarios self-skip, offline scenarios genuinely PASS
- Doctrine established: e2e proofs must be CI-enforced or they rot

## Open Tail

- Branch protection should add all 3 gates (Rust workspace, Android Kotlin, Headless e2e) as required checks to block fleet auto-merge

## Evidence

- transcript lines 10664-10900

