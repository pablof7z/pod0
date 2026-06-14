---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: reversal
status: superseded
subjects:
  - fetch-profile
  - resolved-profiles
  - identity-handler
supersedes:
  - 2026-06-14-3-fetchprofile-is-dead-code-superseded-by
related_claims: []
source_lines:
  - 12109-12113
captured_at: 2026-06-14T08:12:40Z
---

# Episode: FetchProfile is dead code superseded by claimProfile seam

## Prior State

FetchProfile (IdentityAction::FetchProfile in identity_handler.rs:27-29, returning {"status":"nostr_pending"}) was considered a future feature to build — a kind:0 observer that would ingest relay echoes for profile updates, and the natural companion to the #461 self-apply fix.

## Trigger

Cycle-18 planner investigation revealed: kind:0 ingest for any pubkey is already kernel-owned via claimProfile → projections["resolved_profiles"] (android.rs:485-523). The active account's own profile is resolved the same way — iOS claims its own pubkey via UserIdentityStore+ProfileFetch.swift:18. #461's self-echo already round-trips through resolved_profiles + account_profile_interest subscription (register.rs:374). FetchProfile is never called by either shell.

## Decision

FetchProfile is vestigial dead code, superseded by the superior claimProfile/resolved_profiles seam. It should be deleted (~10 lines), not implemented as a feature.

## Consequences

- No kind:0 observer needs to be built — the existing kernel-owned claimProfile seam already handles it
- The 'natural companion to #461' work item is eliminated (was based on a false premise)
- FetchProfile deletion is a trivial cleanup to bundle into any nearby identity PR

## Open Tail

*(none)*

## Evidence

- transcript lines 12109-12113

