---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: active
subjects:
  - kind0-profile-self-apply
  - identity-store
  - account-summary-projection
supersedes:
  - 2026-06-14-1-kind-0-profile-self-apply-saved
  - 2026-06-14-2-kind-0-profile-self-apply-published
related_claims: []
source_lines:
  - 11848-11907
captured_at: 2026-06-14T08:23:24Z
---

# Episode: kind:0 profile self-apply — published profiles now visible on both shells

## Prior State

handle_publish_profile dispatched a kind:0 event to the kernel but never wrote the published display_name/picture_url back into IdentityStore. Since AccountSummary is projected entirely from IdentityStore, the new profile values were invisible in both shells forever — a saved profile simply never appeared in-app.

## Trigger

Diagnosis during cycle-17: IdentityStore only mutates in clear(); display_name/picture_url have no other write path, so a published profile could never surface through the projection.

## Decision

After a successful publish, apply the published fields to IdentityStore via new apply_profile() method (partial-update semantics — only Some fields are written, no nulling-out), persist to disk, and bump_domain(Domain::Identity) at the action site (mirroring the ImportNsec idiom). This ensures AccountSummary immediately reflects the published values on the next push frame.

## Consequences

- Profiles now appear immediately on both iOS and Android after publishing
- The headless scenario asserts AccountSummary.display_name within 2s after dispatch — a CI-gated mutation-proof that would time out without the fix
- Establishes apply→bump_rev→persist as the standard pattern for identity mutations

## Open Tail

- FetchProfile rider deferred — not a self-apply concern but a separate relay-ingest path

## Evidence

- transcript lines 11848-11907

