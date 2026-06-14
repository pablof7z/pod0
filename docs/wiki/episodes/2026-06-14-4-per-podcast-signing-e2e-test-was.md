---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - nipf4-publish-test
  - signing-proof
  - mutation-testing
supersedes:
  - 2026-06-14-2-pr-444-assertion-c-was-false
related_claims: []
source_lines:
  - 10484-10537
captured_at: 2026-06-14T02:04:38Z
---

# Episode: Per-podcast signing e2e test was false confidence — mutation-proofed via sign-and-return

## Prior State

PR #444's assertion C claimed to prove per-podcast signing worked. The test stamped last_published_at + rev.fetch_add unconditionally BEFORE the PublishRaw dispatch, and register_podcast_signer_in_kernel had no failure path. So assertion C passed even with the register call deleted — it only caught an early return before the stamp, never the actual signing seam.

## Trigger

Adversarial review of #444 found that C provided false confidence. Investigation of all offline observables (action_results, publish-outbox, raw-event observer, signed_events sidecar, relay capability) confirmed PublishRaw exposes no pubkey/sig offline. The only network-free seam that proves signing is the D13 sign-and-return path (nmp_app_sign_event_for_return), which resolves the named signer via the identical sign_with_account_nonblocking call.

## Decision

PR #446 rewrote assertion C to drive a sign-and-return on the podcast pubkey, read the signed event from the signed_events push projection via a new sign_tap.rs update-frame callback, and assert pubkey == podcast_pubkey_hex + valid 128-hex sig + 64-hex id + matching kind — for both kind:10154 and kind:54. Mutation-check proof: commenting out register_podcast_signer_in_kernel makes the scenario FAIL with 'no signer for account'.

## Consequences

- The signing proof now genuinely guards the register→sign path
- A new observable (signed_events push projection via sign_tap) is documented and available for future tests
- The 600ms flaky sleep was replaced by sign-and-return's deterministic wait
- Assertion D was honestly downgraded to 'handler ran end-to-end, not a signing proof'

## Open Tail

*(none)*

## Evidence

- transcript lines 10484-10537

