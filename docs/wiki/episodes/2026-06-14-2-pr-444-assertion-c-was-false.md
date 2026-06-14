---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: reversal
status: superseded
subjects:
  - nipf4-publish-e2e
  - signing-proof
  - headless-assertion
supersedes:
  - 2026-06-14-2-per-podcast-signing-proof-false-confidence
related_claims: []
source_lines:
  - 10405-10421
  - 10486-10506
  - 10536-10537
captured_at: 2026-06-14T01:50:50Z
---

# Episode: PR #444 assertion C was false confidence — replaced with sign-and-return observable

## Prior State

Assertion C in the #444 e2e test checked that last_published_at was stamped after publish, which was believed to prove the per-podcast NIP-F4 signing succeeded.

## Trigger

Adversarial review found that host_op_publish.rs:159 stamps last_published_at unconditionally before the PublishRaw dispatch, and register_podcast_signer_in_kernel has no error return — so C passes even if signing is silently broken or the register call is deleted entirely.

## Decision

Replaced assertion C with a sign-and-return observable: after the publish dispatch registers the per-podcast signer, drive nmp_app_sign_event_for_return naming that pubkey, read the signed event from the signed_events push projection (via new sign_tap.rs), and assert pubkey == podcast_pubkey_hex (not the active account), valid 128-hex sig, 64-hex id, matching kind — for both kind:10154 and kind:54. Mutation-check proof: commenting out register_podcast_signer_in_kernel makes the scenario FAIL.

## Consequences

- The e2e test now genuinely guards the register→sign seam (mutation-proven, not just stamp-observed)
- D downgraded honestly to 'handler ran end-to-end, not a signing proof'
- The flaky 600ms sleep replaced by the sign-and-return's deterministic wait
- Module docs corrected to avoid overstating C/D as signing proofs

## Open Tail

*(none)*

## Evidence

- transcript lines 10405-10421
- transcript lines 10486-10506
- transcript lines 10536-10537

