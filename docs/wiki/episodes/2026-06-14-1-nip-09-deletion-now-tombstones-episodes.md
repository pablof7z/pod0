---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: active
subjects:
  - nip09-deletion
  - owned-podcast-lifecycle
  - relay-data-hygiene
supersedes:
  - 2026-06-14-2-nip-09-deletion-must-tombstone-episode
related_claims: []
source_lines:
  - 10688-10695
  - 10747-10773
  - 10814-10817
captured_at: 2026-06-14T03:48:31Z
---

# Episode: NIP-09 deletion now tombstones episodes (k:54), not just the show

## Prior State

delete_owned emitted a kind:5 NIP-09 deletion with only ["k","10154"] (the show kind), leaving all kind:54 episode events published by the per-podcast key orphaned on relays indefinitely

## Trigger

Session-level diagnosis found the gap documented in host_op_publish_lifecycle.rs:215-219 and BACKLOG — a complete vertical-slice delete should remove the entire footprint

## Decision

Emit deletion with both ["k","10154"] AND ["k","54"] tags in a single kind:5 event (NIP-09 permits multiple k tags), via the existing kernel PublishRaw{signer_pubkey} seam — no app-side crypto, D13 preserved

## Consequences

- Owned-podcast deletion now removes the full relay footprint (show + episodes) in one tombstone event
- deletion_tags() helper is independently testable without a live kernel
- Per-podcast-key isolation remains safe: each key authors only that podcast's events
- deletion_event_id remains absent from response per D13

## Open Tail

*(none)*

## Evidence

- transcript lines 10688-10695
- transcript lines 10747-10773
- transcript lines 10814-10817

