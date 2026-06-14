---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - nip09-deletion
  - delete-owned
  - perpodcast-key
supersedes:
  - 2026-06-13-4-nip-09-episode-deletion-incomplete-owned
related_claims: []
source_lines:
  - 10747-10796
captured_at: 2026-06-14T00:00:10Z
---

# Episode: NIP-09 episode deletion — delete_owned now tombstones kind:54 episodes, not just shows

## Prior State

delete_owned emitted a kind:5 NIP-09 deletion with only k:10154 (the show kind). All kind:54 episode events published by the per-podcast key were left orphaned on relays indefinitely. This was a documented gap in BACKLOG.

## Trigger

Identified as a completeness gap in the owned-podcast vertical slice — a full delete should remove everything the slice published. NIP-09 explicitly permits multiple k tags in one kind:5 event.

## Decision

Emit a single kind:5 deletion carrying both k:10154 and k:54 tags, signed via the existing register_podcast_signer_in_kernel + publish_raw_with_signer_via_nmp seam (D13 preserved, no app-side crypto). Introduced testable deletion_tags() helper.

## Consequences

- Owned-podcast deletion tombstones the full footprint (show + episodes) in one event
- Per-podcast key isolation keeps k:54 scoped to that podcast's episodes only
- Unit test asserts both k-tags are present; live-kernel test confirms deletion_status == signed

## Open Tail

*(none)*

## Evidence

- transcript lines 10747-10796

