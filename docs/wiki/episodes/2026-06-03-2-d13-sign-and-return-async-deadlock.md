---
type: episode-card
date: 2026-06-03
session: c43d5e77-d667-4e71-a574-47aaab5b6a7a
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c43d5e77-d667-4e71-a574-47aaab5b6a7a.jsonl
salience: root-cause
status: superseded
subjects:
  - d13-sign-and-return
  - blossom-auth-signing
  - per-podcast-nip-f4
supersedes: []
related_claims: []
source_lines:
  - 8128-8150
  - 8224-8270
  - 8033-8048
captured_at: 2026-06-12T13:02:09Z
---

# Episode: D13 sign-and-return async deadlock — Blossom/per-podcast signing blocked on v0.2.4 API gap

## Prior State

Assumption that nmp_app_sign_event_for_return could be called synchronously from app-Rust host-ops to get a signed event back in-process (e.g. for Blossom kind:24242 HTTP auth headers), and that PublishRaw could accept a signer_pubkey to publish as a non-active account.

## Trigger

Agent investigation of v0.2.4 source code revealed: (1) sign_event_for_return delivers results only through a draining projection frame — no Rust-readable reader exists outside cfg(test-support); (2) app-Rust host-ops run on the NMP actor thread, so an in-Rust await on the result would deadlock; (3) PublishRaw hardcodes signer_pubkey: None — no dispatchable action exposes publish-as-a-specific-account.

## Decision

Per-podcast NIP-F4 (kind:10154/54) and Blossom auth (kind:24242) signing remains in app-Rust for now. Swift-initiated Blossom uploads (avatar, artwork, feedback) degraded with honest throws rather than fake-signed. The gap documented as a v0.2.4 API limitation, not worked around.

## Consequences

- host_op_publish.rs and blossom.rs still use the nostr crate directly — the only remaining app-Rust signing sites
- Blossom uploads from Swift throw 'unavailable' until D13 kernel path is wired (PR #246)
- The sign-for-return result-delivery mechanism (projection-frame drain) is an NMP-internal design constraint that any future wiring must respect — no synchronous bridge from host-ops is possible without NMP changes
- nmp_app_sign_event_for_return IS wired in v0.2.4 (PR #934) per user correction — the blocker is the app-Rust consumption pattern, not the API's existence

## Open Tail

- PR #246 (feat/kernel-sign-and-return) is the active follow-up to wire D13 for Blossom and per-podcast signing
- NMP may need a new host-op-safe consumption path for sign-and-return results (separate from the projection-frame drain)

## Evidence

- transcript lines 8128-8150
- transcript lines 8224-8270
- transcript lines 8033-8048

