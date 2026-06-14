---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - identity-store
  - publish-profile
  - account-summary-projection
supersedes:
  - 2026-06-14-3-kind-0-profile-self-apply-published
related_claims: []
source_lines:
  - 11802-11824
  - 11876-11907
captured_at: 2026-06-14T06:06:13Z
---

# Episode: kind:0 profile self-apply — published profiles now reflect immediately on both shells

## Prior State

`handle_publish_profile` dispatched a kind:0 event to the kernel signer/outbox but never wrote the published `display_name`/`picture_url` back to `IdentityStore`. Since `AccountSummary` is projected entirely from `IdentityStore`, saved profile changes never appeared in-app on either iOS or Android. The ONLY non-test writes to those fields were in `clear()` — setting them to `None`. No kind:0 observer existed (and still doesn't), so even a relay echo wouldn't update the local projection.

## Trigger

The #453 headless scenario investigation revealed that `AccountSummary.display_name`/`picture_url` are populated directly from `id.display_name`/`id.picture_url`, whose only real writes are in `clear()`. The scenario explicitly did NOT assert projection reflection — a coverage hole. The planner confirmed this is a visible, immediate-feedback correctness bug on BOTH shells.

## Decision

After a successful `publish_profile_via_nmp` dispatch, call `id.apply_profile(display_name, picture_url)` on `IdentityStore` — a new method that mutates only `Some` fields (partial-update semantics), persists via `save_to_disk()`, and then `bump_domain(Domain::Identity)` at the real action site (mirroring the ImportNsec pattern) so the push frame re-emits a fresh `AccountSummary`. The headless scenario now asserts `AccountSummary.display_name == TEST_DISPLAY_NAME` within 2s — which times out without the fix, CI-gating the regression.

## Consequences

- Saved profiles now reflect immediately on both shells without waiting for relay echo
- The established `apply→bump_rev→persist` idiom is reused (ImportNsec/Generate/Clear already use it)
- The headless scenario is now a mutation-proof: removing the self-apply causes a timeout failure
- FetchProfile/kind:0 observer remains backlog — self-apply alone fixes the user-visible bug without relay round-trips
- `AccountSummary` shape is unchanged (only its values get populated), so no Android/iOS contract change

## Open Tail

- Wire a kind:0 self-echo observer (`identity_handler.rs:117` FetchProfile stub) to also catch relay echoes and other-users' profiles

## Evidence

- transcript lines 11802-11824
- transcript lines 11876-11907

