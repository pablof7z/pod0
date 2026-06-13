---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - per-podcast-signing
  - blossom-upload
  - nip-f4
  - d13-doctrine
  - podcast-keystore
supersedes:
  - 2026-06-13-2-per-podcast-signing-can-route-through
related_claims: []
source_lines:
  - 8936-8989
  - 9124-9146
  - 9261-9320
captured_at: 2026-06-13T19:33:27Z
---

# Episode: Per-podcast crypto retirement — route signing through kernel seams, delete app-side secp256k1

## Prior State

The app held raw secp256k1 secret keys and performed hand-rolled signing + hand-rolled BUD-02 Blossom upload locally (`blossom.rs::upload_to_blossom`, `host_op_publish.rs::sign_event`). The app's own header comment stated it was waiting for a kernel API to do this.

## Trigger

Spike revealed that the awaited kernel API already exists in the pinned NMP rev: `PublishRaw{signer_pubkey}`, `nmp.blossom.upload{signer_pubkey}`, `sign_with_account_nonblocking`, and `AddSigner{LocalNsec, make_active:false}`. The comment was stale.

## Decision

Delete all app-side crypto (`blossom.rs` module, `sign_event`, `build_auth_event`, `sha256_hex`, `auth_event_tags`, `resolve_episode_tags`, `event_id_from_json`, `publish_via_nmp` scaffold) and route per-podcast NIP-F4 signing through the kernel's existing `signer_pubkey` seams. Also delete dead `blossom.rs`/`blossom_tests.rs` and `hex` dependency.

## Consequences

- D13 doctrine (no raw secret bytes crossing the API) satisfied — keys now live in the kernel keystore, secret transits FFI exactly once for import
- NIP-09 deletion semantics changed from event-ID-targeted (`e` tag) to kind-targeted (`k` tag) — a clean equivalent since a per-podcast key authors exactly one kind:10154 show; no over-deletion possible
- Per-podcast keys temporarily visible in the user's account switcher (upstream NMP lacks a hidden-account flag)
- PodcastKeyStore must remain as seed/re-register source across launches until upstream persists non-active keys
- Upstream NMP issue #1321 filed requesting hidden-account flag and non-active-key persistence
- Registration is idempotent (pubkey-keyed dedup in `IdentityRuntime::add`); FIFO ordering guarantees signer is present before publish dispatch
- Kernel now stamps `created_at`; app's `Utc::now()` only used for local UI `last_published_at`

## Open Tail

- Upstream NMP #1321 (hidden-account + persistence) needed for fully clean retirement
- Episode (kind:54) NIP-09 deletion never existed — pre-existing product gap tracked in BACKLOG
- No end-to-end register→sign integration test against live kernel yet

## Evidence

- transcript lines 8936-8989
- transcript lines 9124-9146
- transcript lines 9261-9320

