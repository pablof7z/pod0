---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - per-podcast-signing
  - nip-f4
  - blossom-upload
  - nip-09-deletion
  - kernel-signer
supersedes:
  - 2026-06-13-2-per-podcast-crypto-retirement-route-signing
related_claims: []
source_lines:
  - 9124-9147
  - 9261-9319
  - 9395-9431
captured_at: 2026-06-13T19:46:29Z
---

# Episode: Route per-podcast NIP-F4 signing through kernel, delete app-side crypto

## Prior State

Owned-podcast publishing signed events locally using raw 32-byte SecretKey from plaintext podcast-keys.json via hand-rolled sign_event, Blossom uploads built kind:24242 auth events in-app, and NIP-09 deletion used event-ID-targeted e-tags (requiring the signed event's ID).

## Trigger

D13 doctrine: kernel owns publish policy and signing. App-side secp256k1 signing with raw secret bytes is a temporal hack that violates the no-hacks mandate.

## Decision

All per-podcast signing routed through kernel seams: register via AddSigner{make_active:false} (idempotent by pubkey dedup), publish via PublishRaw{signer_pubkey}, Blossom via nmp.blossom.upload{signer_pubkey}. Deleted sign_event, build_auth_event, upload_to_blossom, the entire blossom.rs/blossom_tests.rs module, and publish_via_nmp. NIP-09 deletion switched from event-ID-targeted (e-tag) to kind-targeted (k-tag referencing kind:10154) because the kernel signs and does not return the event ID at dispatch time.

## Consequences

- NIP-09 deletion is kind-targeted (k:10154 only); episode kind:54 events are NOT deleted — this is a pre-existing gap, not a regression (old code also only deleted the show event)
- Registration is idempotent: IdentityRuntime::add keys by pubkey hex, re-registration overwrites the identical slot without duplicating the roster order or flipping the active account
- FIFO single-queue ordering ensures the signer is present when the sign-time lookup fires (AddSigner enqueued before PublishRaw on the same host thread)
- created_at is now kernel-stamped (dispatch.rs ctx.kernel.now_secs()) rather than app-set — more D7/D9-compliant
- An unresolved signer produces a recorded terminal failure (fail_publish), not a silent no-op
- Dead code removed in follow-up #438: publish_via_nmp, blossom.rs module (5 tests), unused unsafe block, stale docstrings
- Secret still transits FFI as hex once (kernel import path, wrapped in Zeroizing on entry) — inherent to key import, not app-side signing

## Open Tail

- Per-podcast keys will appear in the account switcher until NMP issue #1321 (hidden-account flag) lands — accepted as temp UX
- Should follow up on whether owned-podcast deletion should also emit k:54 for episode cleanup on relays
- No end-to-end register→sign→publish integration test against a live kernel exists yet (kernel's own suite covers signer_pubkey threading; app tests prove envelope shape with null app)

## Evidence

- transcript lines 9124-9147
- transcript lines 9261-9319
- transcript lines 9395-9431

