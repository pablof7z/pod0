---
type: episode-card
date: 2026-06-12
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - blossom-audio-path
  - nmp-signing-roster
supersedes: []
related_claims: []
source_lines:
  - 6761-6822
  - 6806-6808
captured_at: 2026-06-12T23:56:39Z
---

# Episode: Blossom audio-path migration blocked by upstream NMP signing seam

## Prior State

Blossom audio-path migration was planned for cycle 8, assuming the kernel's `nmp.blossom.upload` action could sign with per-podcast NIP-F4 keys.

## Trigger

Audit of NMP v0.6.2 revealed that `signer_pubkey` in `crates/nmp-core/src/publish/action.rs:129` only selects from registered signers in the kernel identity roster. Per-podcast NIP-F4 keys live in the Podcast-domain `PodcastKeyStore`, which has no API to register them into the kernel roster.

## Decision

Drop Blossom audio-path from cycle 8. File an upstream NMP issue requesting a roster/external-key signing seam instead of burning a lane on blocked work.

## Consequences

- Audio-path uploads still use the legacy Swift-side signing path until upstream provides the seam
- Cycle 8 capacity reallocated to agent-responder (Item A), conversations projection (Item B), and KernelSigner cleanup (Item C)

## Open Tail

- Upstream NMP issue for per-podcast-key roster registration not yet filed

## Evidence

- transcript lines 6761-6822
- transcript lines 6806-6808

