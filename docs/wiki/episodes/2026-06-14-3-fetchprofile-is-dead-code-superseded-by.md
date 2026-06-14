---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: reversal
status: superseded
subjects:
  - fetch-profile
  - identity-handler
  - resolved-profiles
supersedes: []
related_claims: []
source_lines:
  - 12109-12113
captured_at: 2026-06-14T07:49:05Z
---

# Episode: FetchProfile is dead code superseded by claimProfile, not a feature to build

## Prior State

`FetchProfile` was assumed to be a needed future feature — a kind:0 observer that would ingest relay echoes of profile metadata and confirm #461's optimistic apply. It was the natural next candidate after the self-apply fix.

## Trigger

Cycle-18 planner investigation revealed that kind:0 ingest for any pubkey (including the active account's own) is already kernel-owned via `claimProfile` → `projections["resolved_profiles"]`; iOS already claims its own pubkey via `claimProfile(consumerID:"UserIdentityStore.ownProfile")`; and #461's self-echo already round-trips through the standing `account_profile_interest` subscription. The host-op `FetchProfile` returning `{"status":"nostr_pending"}` is never called by either shell.

## Decision

FetchProfile is vestigial dead code superseded by a superior kernel-owned seam. The action is to delete the ~10-line stub, not build a feature around it.

## Consequences

- No new FetchProfile/kind:0-observer implementation — the capability already exists via claimProfile/resolved_profiles
- The stub should be deleted as a trivial cleanup, folded into any nearby identity PR
- The 'confirm optimistic apply via relay echo' path already works through resolved_profiles + account_profile_interest

## Open Tail

*(none)*

## Evidence

- transcript lines 12109-12113

