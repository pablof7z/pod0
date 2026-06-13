---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - per-podcast-crypto
  - blossom-upload
  - signer-pubkey
  - nmp-identity-roster
supersedes:
  - 2026-06-12-3-blossom-audio-path-migration-blocked-by
  - 2026-06-03-2-d13-sign-and-return-async-deadlock
related_claims: []
source_lines:
  - 8869-8981
  - 8988-9010
captured_at: 2026-06-13T18:48:50Z
---

# Episode: Per-podcast signing can route through existing kernel seam; retire app-side crypto

## Prior State

The app believed it was waiting for a kernel 'sign-as-non-active-account' API (stale comment at host_op_publish.rs:1-10). It held raw secp256k1 secret bytes in app code and hand-rolled the entire BUD-02 Blossom upload pipeline (blossom.rs) + per-podcast event signing (host_op_publish.rs::sign_event), duplicating what the kernel already provides.

## Trigger

Spike investigation revealed the API already exists in the pinned NMP rev: sign_with_account_nonblocking (identity.rs:787), PublishRaw.signer_pubkey (publish/action.rs:142), nmp.blossom.upload.signer_pubkey (blossom/action.rs:38), and AddSigner{make_active:false} (identity.rs:948-971). The app's own header comment was stale.

## Decision

Retire app-side crypto now by registering per-podcast keys as inactive signers (AddSigner{make_active:false}) and routing all per-podcast publish + Blossom upload through PublishRaw{signer_pubkey} / nmp.blossom.upload{signer_pubkey}. Accept temporary cosmetic regression (per-podcast keys appear in user's account switcher) until upstream adds a hidden-account flag. File upstream NMP issue requesting hidden-account classification + non-active-key persistence.

## Consequences

- Deletes blossom.rs hand-rolled upload and host_op_publish.rs::sign_event — removes all raw secp256k1/secret-bytes from app code
- PodcastKeyStore must remain as seed/re-register source across kernel restarts until upstream persistence lands (NMP only persists the active account)
- Upstream issue pablof7z/nostr-multi-platform#1321 filed requesting hidden-account flag + non-active-key persistence
- Per-podcast keys will temporarily surface in account switcher until upstream M1 lands — accepted as interim UX tradeoff
- The BACKLOG truthfulness sweep also corrected a related belief: social-publish was thought to hardcode relay.primal.net but actually routes via nmp_dispatch target:Auto (pool-aware), shrinking the remaining social-publish work to a verification spike

## Open Tail

- Upstream NMP #1321 (hidden-account flag + persistence) blocks fully clean PodcastKeyStore retirement
- App-side stopgap for account-switcher pollution: could hide per-podcast keys in Swift account UI temporarily

## Evidence

- transcript lines 8869-8981
- transcript lines 8988-9010

