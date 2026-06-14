---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - nip-09
  - delete-owned
  - kind-54
supersedes:
  - 2026-06-13-3-nip-09-episode-deletion-delete-owned
related_claims: []
source_lines:
  - 10747-10818
captured_at: 2026-06-14T00:26:33Z
---

# Episode: NIP-09 deletion must tombstone episode (kind:54) events, not just the show

## Prior State

delete_owned emitted only a kind:5 deletion with ["k","10154"] (show kind), leaving all kind:54 episode events orphaned on relays indefinitely

## Trigger

Identified gap in the owned-podcast vertical slice — documented in host_op_publish_lifecycle docstring and BACKLOG perpodcast-publish-followups (b)

## Decision

Single multi-k kind:5 deletion carrying both ["k","10154"] and ["k","54"] tags, routed via the same kernel register_podcast_signer_in_kernel + publish_raw_with_signer_via_nmp seam (D13 preserved, no app-side crypto). Introduced deletion_tags() helper returning both k-tags, tested independently

## Consequences

- Owned-podcast deletion now tombstones the full footprint (show + episodes) in one event
- Per-podcast-key isolation ensures k:54 is scoped to that podcast's episodes only
- deletion_event_id remains absent from response (no contract change)

## Open Tail

*(none)*

## Evidence

- transcript lines 10747-10818

