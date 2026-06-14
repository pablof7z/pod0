---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: reversal
status: active
subjects:
  - fetch-profile
  - claim-profile
  - resolved-profiles
supersedes:
  - 2026-06-14-2-fetchprofile-is-dead-code-superseded-by
related_claims: []
source_lines:
  - 12107-12126
captured_at: 2026-06-14T08:23:24Z
---

# Episode: FetchProfile is vestigial dead code — planned feature replaced by superior seam

## Prior State

FetchProfile was a planned feature to implement a kind:0 observer so the app could ingest relay self-echoes and hydrate the active account profile — it was the assumed next step after #461's self-apply fix.

## Trigger

Cycle-18 planner analysis found that kind:0 ingest for any pubkey (including the active account's own) is already handled by claimProfile → projections["resolved_profiles"]. iOS already claims its own pubkey via UserIdentityStore+ProfileFetch. The self-echo confirmation of #461's optimistic apply already round-trips through resolved_profiles + the standing account_profile_interest subscription.

## Decision

Do NOT build FetchProfile as a feature. IdentityAction::FetchProfile (which returns {"status":"nostr_pending"}) is vestigial dead code superseded by the claimProfile seam. Delete it as a trivial cleanup, not a feature cycle.

## Consequences

- Eliminates a planned feature cycle — no new kernel seam needed for kind:0 observation
- claimProfile/resolved_profiles is the canonical path for all profile resolution, including self
- The FetchProfile stub should be removed as a minor cleanup bundled into a nearby PR

## Open Tail

- FetchProfile deletion not yet done — noted as a trivial cleanup to bundle

## Evidence

- transcript lines 12107-12126

