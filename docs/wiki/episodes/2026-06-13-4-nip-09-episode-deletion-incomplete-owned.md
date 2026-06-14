---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - nip-09-deletion
  - delete-owned
  - episode-orphan
supersedes:
  - 2026-06-13-2-route-per-podcast-nip-f4-signing
related_claims: []
source_lines:
  - 10689-10695
captured_at: 2026-06-13T23:44:31Z
---

# Episode: NIP-09 episode deletion incomplete — owned-podcast delete orphans kind:54 events on relays

## Prior State

delete_owned emitted only a kind:5 deletion targeting ["k","10154"] (shows), leaving published kind:54 (episode) events orphaned on relays forever.

## Trigger

Code investigation of host_op_publish_lifecycle.rs:226-290 found the deletion only carries a single kind tag for shows, documented as a gap in BACKLOG perpodcast-publish-followups.

## Decision

Emit a single kind:5 deletion carrying both ["k","10154"] and ["k","54"] tags signed by the per-podcast pubkey, tombstoning the full owned-podcast footprint in one event. The per-podcast key is still in hand at delete time.

## Consequences

- Delete means delete — the full vertical-slice footprint (shows + episodes) is removed from relays
- No FFI shape change needed (delete_owned's return already omits deletion_event_id per D13)
- Pairs with the new headless CI gate so nipf4_publish can assert the kind:54 tag in e2e

## Open Tail

- Implementation dispatched but not yet merged at session end

## Evidence

- transcript lines 10689-10695

