---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - nipf4-publish-e2e
  - signing-seam-assertion
  - headless-scenario
supersedes:
  - 2026-06-13-3-per-podcast-publish-e2e-test-had
related_claims: []
source_lines:
  - 10364-10519
captured_at: 2026-06-14T00:00:10Z
---

# Episode: E2e signing assertion was false confidence — strengthened to mutation-proof signed-event observable

## Prior State

PR #444's headline assertion C claimed last_published_at stamped ⇒ signing succeeded, presenting the e2e test as proof that the per-podcast NIP-F4 register→sign→publish seam was guarded.

## Trigger

Opus review found host_op_publish.rs:159 stamps last_published_at unconditionally before the PublishRaw dispatch, and register_podcast_signer_in_kernel has no error return — so assertion C passes even if the register call is deleted entirely. The test gave false confidence.

## Decision

Replace assertion C with a sign-and-return observable: drive nmp_app_sign_event_for_return naming the per-podcast pubkey, read the signed event from the signed_events push projection via sign_tap.rs, and assert pubkey == podcast_pubkey_hex + valid 128-hex sig + matching kind. Mutation-check proof: commenting out register_podcast_signer_in_kernel makes the scenario FAIL.

## Consequences

- The e2e test now genuinely guards the register→sign path (mutation-proven)
- Assertion D downgraded honestly to 'handler ran end-to-end, not a signing proof'
- headless scenarios still not in CI (separate follow-up)

## Open Tail

*(none)*

## Evidence

- transcript lines 10364-10519

