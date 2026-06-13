---
title: Podcast Keys Storage
slug: podcast-keys-storage
topic: data-persistence
summary: Podcast keys are stored in podcast-keys.json only; there is no Keychain storage for them
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-03
updated: 2026-06-13
verified: 2026-06-03
compiled-from: conversation
sources:
  - session:55bedfc3-dd9e-4b1c-b7d7-cea0c699d4d1
  - session:rollout-2026-05-25T12-53-39-019e5e8d-ec64-74f1-a1b1-91055dcab442
  - session:rollout-2026-05-25T12-53-46-019e5e8e-043d-7dc2-8171-2238de03d145
  - session:rollout-2026-05-26T09-10-44-019e62e8-3112-7080-98ca-48ac46d8b8d2
  - session:rollout-2026-05-26T10-16-10-019e6324-1915-7ba0-91ff-8397304bb76a
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Podcast Keys Storage

## Podcast Keys Storage

Podcast keys are stored in podcast-keys.json only; there is no Keychain storage for them. The PodcastKeysKeychainMigration and all legacy migration infrastructure (LegacyKeychainMigration, LegacyIOCapability, LegacyIOTypes) are deleted. There is no legacy v1 shipped app to migrate from; all legacy migration infrastructure was dead from day one and has been removed. Per-podcast NIP-F4 events (kind:10154/54) are signed with per-podcast secp256k1 keys registered as non-active NMP accounts via nmp_app_create_new_account(make_active: false); the app does not sign these directly. (Previously: Show/episode events and Blossom uploads must be signed with the podcast key, superseded — see nip74-episode-events.) (Previously: Owned-podcast keys must use real secp256k1 key derivation and persistence, not in-memory placeholder FNV pubkey derivation, superseded — see nip74-episode-events.) Per-podcast key registration via AddSigner{make_active:false} is idempotent: IdentityRuntime::add keys by pubkey hex, so re-registering the same key overwrites without duplicating roster entries or changing the active account. The NMP v0.6.2 seam sign_with_account_nonblocking already exists and resolves per-podcast keys by pubkey hex across both local-key and remote-signer maps, independent of the active account — the app's own header comment saying this capability is missing was stale. Per-podcast NIP-F4 signing can partially move to the kernel now via `AddSigner { LocalNsec, make_active: false }` + `PublishRaw.signer_pubkey` / `nmp.blossom.upload.signer_pubkey`, but a fully clean retirement requires upstream NMP changes: a hidden-account flag (M1) and non-active-key persistence (M2). Upstream NMP issue #1321 was filed requesting an app-managed/hidden-account flag and persistence for non-active local keys to enable clean retirement of the app's PodcastKeyStore. Until it lands, PodcastKeyStore stays as the seed/re-register source and per-podcast keys appear in the account list as a temporary UX regression. The blossom-audio-path migration remains blocked because per-podcast NIP-F4 keys live in the Podcast-domain PodcastKeyStore, not in the NMP account roster that signer_pubkey resolves against, and there is no API to register per-podcast keys as named roster accounts. Publishing must actually sign and broadcast events to relays, not return relay_pending without signing/broadcasting. PodcastKeyStoreTests must cover save, read, delete cleanup, and per-podcast isolation using unique UUIDs. The Podcast model includes an ownerPubkeyHex field identifying agent-owned shows; this field is set to the podcast's own pubkey, not the agent pubkey. (Previously: Existing public podcasts have agent pubkeys stored; migration needs a backfill or publish-time conversion path, superseded — see agent-owned-podcasts.)

<!-- citations: [^55bed-14] [^rollo-185] [^rollo-199] [^rollo-230] [^rollo-254] [^c1691-199] [^c1691-214] [^c1691-260] [^c1691-292] [^c1691-302] -->
