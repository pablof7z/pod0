---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - nipf4-publish-e2e
  - signing-proof
  - false-confidence-test
supersedes:
  - 2026-06-13-3-e2e-test-stamp-based-assertions-are
related_claims: []
source_lines:
  - 10364-10511
captured_at: 2026-06-13T23:44:31Z
---

# Episode: Per-podcast publish e2e test had false confidence — assertion C proved the stamp, not signing

## Prior State

PR #444's headline assertion C (last_published_at stamped ⇒ signing succeeded) was believed to guard the per-podcast NIP-F4 signing seam.

## Trigger

Opus review found that host_op_publish.rs:159 stamps last_published_at unconditionally before the PublishRaw dispatch, and register_podcast_signer_in_kernel (nmp_dispatch.rs:44) has no error return — so C passes even if signing is deleted entirely.

## Decision

Replace assertion C with the D13 sign-and-return seam (nmp_app_sign_event_for_return): drive a sign-and-return naming the per-podcast pubkey, read the signed event from the signed_events push projection via a new sign_tap.rs helper, and assert pubkey == podcast_pubkey_hex + valid 128-hex sig + valid id + matching kind for both kind:10154 and kind:54. Mutation-check proof: commenting out register_podcast_signer_in_kernel → scenario FAILS.

## Consequences

- The e2e test now genuinely guards the per-podcast signing seam — false confidence eliminated
- Downgrade D honestly to 'handler ran end-to-end, not a signing proof'
- Merged as PR #446 (commit 7c811e69), strengthening the previously-merged #444

## Open Tail

*(none)*

## Evidence

- transcript lines 10364-10511

