---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - identity-store
  - profile-self-apply
  - account-summary-projection
supersedes: []
related_claims: []
source_lines:
  - 11791-11907
captured_at: 2026-06-14T08:12:40Z
---

# Episode: kind:0 profile self-apply: saved profiles never appeared in-app

## Prior State

After a user edits and saves their profile, the published name/picture never appear in-app on either iOS or Android. handle_publish_profile dispatched a kind:0 event to the kernel signer/outbox but never wrote the published fields back into IdentityStore. AccountSummary.display_name/picture_url were projected entirely from IdentityStore — whose only non-test writes to those fields were in clear() (setting them to nothing). No kind:0 observer existed to catch a relay echo either.

## Trigger

Cycle-17 root-cause investigation: traced handle_publish_profile → publish_profile_via_nmp → kernel signer/outbox, confirmed it never touches IdentityStore. Grepped for any kind:0 ingest/Kind::Metadata/on_metadata — zero hits across apps/nmp-app-podcast/src. Both shells depend on this projection (Android EditProfileScreen reads account?.pictureUrl/displayName from PodcastSnapshot.activeAccount).

## Decision

After a successful publish, locally apply the published fields to IdentityStore via new apply_profile(display_name, picture_url) method (partial-update semantics, save_to_disk(), bump identity rev so push frame re-emits AccountSummary). Added bump_domain(Domain::Identity) at the real action site in social_actions.rs (PublishProfile arm), mirroring the ImportNsec bump pattern. Uses the established apply→bump_rev→persist idiom.

## Consequences

- Saved profiles now reflect immediately on both iOS and Android (real both-shells correctness fix)
- Headless profile.rs scenario now asserts AccountSummary.display_name == published value within 2s — mutation-proof CI guard that times out without the fix
- Contract-safe: AccountSummary shape unchanged, only its values get populated
- Rust-only fix with no Android-Gradle/Xcode disk cost
- FetchProfile stub identified as vestigial dead code superseded by claimProfile/resolved_profiles

## Open Tail

- FetchProfile deletion (dead code, ~10 lines) can be bundled into a future identity PR
- A real kind:0 self-echo observer could also ingest others' profiles via claimProfile (future enhancement, not blocking)

## Evidence

- transcript lines 11791-11907

