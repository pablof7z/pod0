---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - nip-09-deletion
  - owned-podcast
  - kind-54
supersedes:
  - 2026-06-14-3-nip-09-deletion-now-tombstones-episodes
related_claims: []
source_lines:
  - 10689-10696
  - 10747-10772
  - 10877-10896
captured_at: 2026-06-14T02:04:38Z
---

# Episode: NIP-09 deletion must tombstone episodes (kind:54), not just the show

## Prior State

delete_owned emitted a kind:5 NIP-09 deletion event with only ["k","10154"] (the show kind). All kind:54 episode events published by the per-podcast key were left orphaned on relays indefinitely. This was a known gap documented in the BACKLOG and host_op_publish_lifecycle.rs docstrings.

## Trigger

Cycle-14 planner identified the gap and the D13 kernel-signing seam already provides the per-podcast pubkey at delete time, making the fix straightforward.

## Decision

Emit a single kind:5 deletion carrying both ["k","10154"] and ["k","54"] tags (NIP-09 permits multiple k tags). A new deletion_tags() helper builds the two-element list; the existing register_podcast_signer_in_kernel + publish_raw_with_signer_via_nmp seam handles signing. Per-podcast-key isolation keeps the scope correct.

## Consequences

- Owned-podcast deletion now removes the full footprint (show + episodes) from relays
- deletion_tags() is unit-testable without a live kernel
- The D13 no-app-side-crypto doctrine is preserved — signing still routes through the kernel

## Open Tail

*(none)*

## Evidence

- transcript lines 10689-10696
- transcript lines 10747-10772
- transcript lines 10877-10896

