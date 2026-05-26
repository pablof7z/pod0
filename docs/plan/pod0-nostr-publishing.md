# Plan: Pod0 Nostr Publishing Migration From NIP-74 To NIP-F4

Canonical status: tracked from `docs/plan.md` and `docs/BACKLOG.md`.

## Current Status - 2026-05-26

This migration is **not complete**. PR #89 corrected the active
show/episode builders and show parser away from the known NIP-74-era wire
shape, and PR #93 replaced fake public-key derivation with real secp256k1
key generation. The remaining work is not "add more screens"; it is replacing
the scaffolded publish/discovery path with persisted keys, signed events,
relay publication, relay-backed discovery, author claims, legacy-data
handling, and validation against real relays.

The repo still has legacy names and old parser paths (`NIP74Show`,
`NIP74Episode`, `parse_episode_event`, and related comments/tests) alongside
the newer `NipF4Show` discovery parser and corrected publish builders. Treat
those as migration debt until each path is either renamed/reworked for NIP-F4
or explicitly quarantined as legacy compatibility.

## Required NIP-F4 Contract

| Concept | Required NIP-F4 Shape |
|---|---|
| Show event | `kind:10154`, signed by the podcast key, no `d` tag |
| Episode event | `kind:54`, signed by the podcast key, no `d` tag |
| Author claim | `kind:10064`, signed by the agent key, `p` tags for podcast pubkeys |
| Show coordinate | `10154:<podcast-pubkey-hex>` |
| Episode identity | Event id, scoped under podcast pubkey |
| Show description | `["description", "..."]` plus content fallback |
| Episode audio | `["audio", "<url>", "<mime>"]` |
| Episode-to-show link | Implicit by author pubkey; no `a` tag |
| Published time | Event `created_at`; no `published_at` tag |
| Key storage | Real secp256k1 private key per podcast, persisted securely |

## Completed Corrections

- `apps/podcast-discovery/src/build/show.rs` no longer emits a show `d` tag
  and writes `description` for the publish builder path.
- `apps/podcast-discovery/src/build/episode.rs` no longer emits episode `d`,
  `a`, or `published_at` tags for the publish builder path.
- `apps/podcast-discovery/src/build/episode.rs` now writes `description` and
  `audio` tags for the publish builder path instead of `summary` and `imeta`.
- `apps/podcast-discovery/src/parse/show.rs` maps show coordinates as
  `10154:<podcast-pubkey>` with no `d` suffix.
- `apps/nmp-app-podcast/src/store/podcast_keys.rs` now uses
  `nostr::Keys::generate()` / secp256k1 key material for per-podcast pubkey
  derivation.

## Remaining Wrong Or Scaffolded Behavior

- `PodcastKeyStore` keeps per-podcast keys in memory only. The next slice must
  persist secrets in the native secure store, reload them after restart, and
  remove them when owned podcasts are deleted.
- `apps/nmp-app-podcast/src/host_op_publish.rs` still builds unsigned
  diagnostic event JSON with `id: null` and `sig: null`.
- `apps/nmp-app-podcast/src/host_op_publish.rs` still returns
  `status: "relay_pending"` without a durable publish queue or relay
  acceptance. UI copy must not imply a podcast is actually published yet.
- `publish_author_claim` builds unsigned kind `10064` diagnostics but does
  not sign, publish, refresh, or reconcile the author claim against relay data.
- `podcast.discover_nostr` still uses an HTTP gateway/search response parser,
  not a canonical relay subscription path.
- Pure-Nostr subscription to a show/episode set is incomplete; discovery still
  leans on `feed` when present and RSS subscribe for the durable path.
- `apps/podcast-discovery/src/nip_f4.rs` still parses the older
  PR-19-style `summary` tag for gateway discovery. Decide whether this remains
  accepted legacy input or is migrated to the `description` tag used by the
  active publish builder.
- Legacy parser/type paths still carry NIP-74 concepts:
  `NIP74Show`/`NIP74Episode`, episode `d_tag`, `published_at`, `imeta`, and
  `show_a_tag`. Each caller must either move to canonical NIP-F4 event-id
  identity and `audio` tags or be explicitly marked legacy read-only.
- Source comments in `apps/podcast-discovery/src/kinds.rs`, `lib.rs`, and the
  old parser modules still describe mixed NIP-74/NIP-F4 semantics. Clean them
  up in the same PR that rewires or quarantines those code paths.
- Existing local rows with old `30074:<pubkey>:<d>` coordinates or
  NIP-74-derived metadata still need an explicit migrate/hide/read-only policy.

## P0 Implementation Plan

1. Rename or quarantine the remaining legacy typed raw views away from
   `NIP74Show`/`NIP74Episode`. Use `NipF4Show` and `NipF4Episode`
   consistently for canonical NIP-F4 parse/build/tests; keep any NIP-74
   compatibility path visibly legacy.
2. Finish episode parsing for canonical NIP-F4.
   Remove required `d`; remove `a`; remove `published_at`; read/write
   `description`; read/write `audio`; identify episodes by event id under the
   podcast pubkey. Keep legacy `imeta`/`summary`/`d` parsing only behind an
   explicit compatibility label if still required.
3. Update tests to fail on NIP-74 tags in the active publish/discovery path.
   Builder tests already cover much of this; extend parse/discovery tests so
   `d`, `a`, `summary`, `published_at`, and `imeta` cannot re-enter the
   canonical path accidentally.
4. Persist per-podcast key material.
   PR #93 fixed generation/public-key derivation; now store each podcast
   private key in the proper Keychain/native secure-store slot, survive
   restart, and delete the secret when ownership is removed.
5. Sign events in Rust.
   Build full Nostr events with valid `id`, `pubkey`, `sig`, `created_at`,
   content, and tags before returning success.
6. Publish events to relays.
   Use the configured relay list/NIP-65 write relays. `relay_pending` may only
   remain as an intermediate queue state if a durable queue exists.
7. Publish and maintain author claims.
   After create/update/delete of owned podcasts, publish kind `10064` from the
   agent key with the current set of podcast pubkeys.
8. Implement relay-backed discovery and episode fetch.
   Query kind `10154` for shows and kind `54` by podcast pubkey for episodes.
   HTTP gateway search can remain a convenience wrapper, but the canonical
   path must be relay/substrate-backed.
9. Update iOS/Android UI semantics.
   Owned-podcast UI must not tell users a show/episode is published until a
   signed event has been accepted or queued durably for relay publish.
10. Add migration handling for existing NIP-74-derived local data.
   Existing `nostr_coordinate` and `owner_pubkey_hex` rows must either be
   migrated to NIP-F4 coordinates or explicitly treated as legacy read-only
   entries.
11. Validate against relays.
   Publish a show, publish an episode, fetch both back from relay data, verify
   author claim, delete/cleanup key material, and confirm restart behavior.

## Done Criteria

- No active NIP-F4 publish/build test expects `d`, `a`, `summary`,
  `published_at`, or `imeta`.
- `KIND_SHOW == 10154`, `KIND_EPISODE == 54`, and
  `KIND_AUTHOR_CLAIM == 10064` are still pinned in one canonical module.
- Per-podcast keys survive restart and derive real secp256k1 pubkeys.
- `publish_show`, `publish_episode`, and `publish_author_claim` produce signed
  events and publish them to relays.
- Discovery can subscribe to a Nostr-only podcast without relying on RSS.
- Docs, tests, UI copy, and `whats-new.json` no longer describe NIP-74 as the
  current publish/discovery protocol.
