---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - e2e-test-doctrine
  - nipf4-publish-scenario
  - sign-and-return-seam
supersedes: []
related_claims: []
source_lines:
  - 10364-10421
  - 10481-10511
captured_at: 2026-06-13T22:49:46Z
---

# Episode: E2e test stamp-based assertions are false confidence — must assert on real effects

## Prior State

The nipf4_publish e2e scenario's headline assertion C used last_published_at as a proxy for "the kernel signed with the per-podcast key." This gave false confidence because host_op_publish.rs stamps last_published_at unconditionally at line 159 BEFORE the PublishRaw dispatch, and register_podcast_signer_in_kernel has no error return — so C passes even if signing is deleted entirely.

## Trigger

Opus review of PR #444 found that assertion C proves the handler reached the stamp, not that signing succeeded. Deleting the register_podcast_signer_in_kernel call entirely would still make C pass. This is the exact false-confidence risk the durable mandate guards against.

## Decision

Rewrite assertion C to use the D13 sign-and-return seam (nmp_app_sign_event_for_return), which resolves the named signer via the identical sign_with_account_nonblocking path that PublishRaw uses. Assert the signed event's pubkey == podcast_pubkey_hex, valid 128-hex sig, and 64-hex id for both kind:10154 and kind:54. Require mutation-check proof (comment out register call → test must FAIL).

## Consequences

- Test now genuinely guards the register→sign seam — mutation-proven
- Sign-and-return is now the canonical offline observable for proving per-podcast signing
- Headless scenarios are not yet in CI, so this guard is on-demand only (follow-up: wire network-free headless into CI)
- Future e2e assertions must assert on real effects (signed events, dispatched payloads), not proxy timestamps or correlation IDs

## Open Tail

- Wire network-free headless scenarios into CI (4 other scenarios currently timeout in sandbox, need Skip gates)
- Headless scenarios still not in CI — protection is on-demand only until CI gating is added

## Evidence

- transcript lines 10364-10421
- transcript lines 10481-10511

