---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - nip09-episode-deletion
  - owned-podcast-delete
supersedes:
  - 2026-06-14-2-nip-09-deletion-must-tombstone-episodes
related_claims: []
source_lines:
  - 10689-10695
  - 10747-10773
  - 10796-10818
captured_at: 2026-06-14T02:13:47Z
---

# Episode: NIP-09 deletion must tombstone episode (kind:54) events, not just the show

## Prior State

delete_owned emitted only a kind:5 deletion with ["k","10154"] (show kind), leaving all kind:54 episode events orphaned on relays indefinitely

## Trigger

Identified as a known gap in host_op_publish_lifecycle.rs docstring and BACKLOG — published episodes have no deletion path

## Decision

Emit a single kind:5 deletion carrying both ["k","10154"] and ["k","54"] tags via the per-podcast key's PublishRaw seam (NIP-09 permits multiple k tags), tombstoning the full owned-podcast footprint in one event

## Consequences

- Complete owned-podcast deletion footprint — episodes no longer orphaned
- deletion_tags() helper is independently testable without a live kernel
- Per-podcast-key isolation means k:54 tag is scoped to only that podcast's episodes
- Routed through existing kernel PublishRaw seam — no app-side crypto

## Open Tail

*(none)*

## Evidence

- transcript lines 10689-10695
- transcript lines 10747-10773
- transcript lines 10796-10818

