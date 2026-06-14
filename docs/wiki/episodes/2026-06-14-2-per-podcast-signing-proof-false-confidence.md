---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: reversal
status: superseded
subjects:
  - nipf4-publish-test
  - signing-seam
  - e2e-assertion
supersedes:
  - 2026-06-13-2-e2e-signing-assertion-was-false-confidence
related_claims: []
source_lines:
  - 10291-10537
captured_at: 2026-06-14T00:26:33Z
---

# Episode: Per-podcast signing proof: false-confidence assertion replaced by mutation-proven check

## Prior State

PR #444's headline assertion C (last_published_at stamped) was believed to prove per-podcast NIP-F4 signing succeeded

## Trigger

Adversarial review found that host_op_publish.rs:159 stamps last_published_at unconditionally BEFORE the publish dispatch, and register_podcast_signer_in_kernel has no error return — so C passes even if signing is entirely deleted

## Decision

PR #446 replaced assertion C with a sign-and-return seam (nmp_app_sign_event_for_return) that resolves the named signer via the identical sign_with_account_nonblocking call PublishRaw uses. New C asserts the signed event's pubkey == podcast_pubkey_hex, valid 128-hex sig, 64-hex id, matching kind — for both kind:10154 and kind:54. Mutation-proven: deleting register_podcast_signer_in_kernel makes the test FAIL with 'no signer for account'

## Consequences

- The signing seam is now genuinely guarded — no more false confidence from stamp-based assertion
- Assertion D honestly downgraded to 'handler ran end-to-end, not a signing proof'
- The D13 sign-and-return path is the only network-free observable that proves kernel signing; PublishRaw exposes no pubkey/sig offline

## Open Tail

- Headless scenarios still not in CI at time of #446 (resolved by #449 later in session)

## Evidence

- transcript lines 10291-10537

