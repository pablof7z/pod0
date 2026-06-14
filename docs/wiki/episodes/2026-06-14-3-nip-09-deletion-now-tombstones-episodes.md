---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - nip09-deletion
  - delete-owned
  - episode-events
supersedes:
  - 2026-06-14-3-nip-09-deletion-must-tombstone-episode
related_claims: []
source_lines:
  - 10747-10767
  - 10779-10784
captured_at: 2026-06-14T01:50:50Z
---

# Episode: NIP-09 deletion now tombstones episodes (kind:54), not just the show

## Prior State

delete_owned emitted a kind:5 NIP-09 deletion with only ["k","10154"] (the show kind). All kind:54 episode events published by the per-podcast key were left orphaned on relays indefinitely.

## Trigger

Identified gap in host_op_publish_lifecycle.rs and BACKLOG perpodcast-publish-followups (b): a complete vertical-slice delete should remove the full footprint, not just the show.

## Decision

delete_owned now emits a single kind:5 deletion carrying both ["k","10154"] and ["k","54"] tags (NIP-09 permits multiple k tags), signed via the same kernel PublishRaw{signer_pubkey} seam. Introduced pub(crate) fn deletion_tags() → Vec<Vec<String>> for testability.

## Consequences

- Owned-podcast deletion is now complete — relay tombstoning covers the full footprint
- Per-podcast key isolation ensures k:54 is scoped to that podcast's episodes only
- No FFI shape change — delete_owned's return already omitted deletion_event_id
- Unit test delete_owned_nip09_covers_show_and_episodes asserts both k-tags

## Open Tail

- Once headless e2e is CI-gated, nipf4_publish scenario could add an e2e assertion that delete emits the kind:54 tag

## Evidence

- transcript lines 10747-10767
- transcript lines 10779-10784

