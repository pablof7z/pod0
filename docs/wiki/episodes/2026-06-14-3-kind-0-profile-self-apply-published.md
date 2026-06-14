---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - publish-profile
  - identity-store
  - account-summary-projection
supersedes: []
related_claims: []
source_lines:
  - 11800-11823
  - 11848-11855
captured_at: 2026-06-14T05:49:17Z
---

# Episode: kind:0 profile self-apply — published profile never appears in-app on either shell

## Prior State

handle_publish_profile signs and queues a kind:0 event to relays but never locally applies the published fields to IdentityStore; AccountSummary.display_name/picture_url are populated from id.display_name/id.picture_url whose only non-test writes are in clear(), so a just-saved profile never appears in-app on either iOS or Android until a relay echo that has no wired observer

## Trigger

#453's headless scenario investigation discovered that handle_publish_profile never touches IdentityStore; planner verified zero kind:0 ingest/self-echo observers exist in the app — even a relay echo wouldn't update the projection

## Decision

After successful publish, locally apply the published fields (display_name, picture_url) to IdentityStore via a new setter (e.g. apply_profile), persist with save_to_disk(), and bump the identity rev so the push frame re-emits AccountSummary immediately. Uses the established apply→bump_rev→persist idiom already present in identity_handler.rs (ImportNsec/Generate/Clear)

## Consequences

- Both shells immediately reflect a just-published profile change without waiting for relay round-trips
- AccountSummary shape unchanged — only its values get populated
- Headless scenario can add an assertion that activeAccount.display_name reflects after publish, closing the coverage hole
- FetchProfile stub at identity_handler.rs:117 is the natural home for a future kind:0 self-echo observer

## Open Tail

- Implementation launched (cycle-17 #1) but not yet merged
- A kind:0 self-echo observer (wiring FetchProfile) is a natural follow-up but not required for the self-apply fix

## Evidence

- transcript lines 11800-11823
- transcript lines 11848-11855

