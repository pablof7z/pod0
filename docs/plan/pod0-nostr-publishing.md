# Plan: Pod0 Nostr Publishing Migration From NIP-74 To NIP-F4

Canonical status: tracked from `docs/plan.md` and `docs/BACKLOG.md`.

## Current Status - 2026-05-26

This migration is **not complete**. The repository has merged NIP-F4 discovery
and owned-podcast publishing scaffolds, but the current wire/build path still
contains NIP-74-era assumptions. Treat this as a P0 protocol-correction plan,
not a finished implementation note.

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

## Current Wrong Or Scaffolded Behavior

- `apps/podcast-discovery/src/build/show.rs` still emits a show `d` tag.
- `apps/podcast-discovery/src/build/show.rs` still emits `summary` instead of
  `description`.
- `apps/podcast-discovery/src/build/episode.rs` still emits an episode `d` tag.
- `apps/podcast-discovery/src/build/episode.rs` still emits `published_at`.
- `apps/podcast-discovery/src/build/episode.rs` still emits an `a` tag linking
  to a show coordinate.
- `apps/podcast-discovery/src/build/episode.rs` still emits `summary` instead
  of `description`.
- `apps/podcast-discovery/src/build/episode.rs` still emits `imeta` instead of
  the required `audio` tag.
- `apps/podcast-discovery/src/types.rs` still names parsed views
  `NIP74Show`/`NIP74Episode` and carries `d_tag`/`show_a_tag` fields.
- `apps/podcast-discovery/src/parse/show.rs` still requires a `d` tag and
  computes `10154:<pubkey>:<d-tag>` coordinates.
- `apps/podcast-discovery/src/parse/episode.rs` still requires a `d` tag and
  parses an `a` show reference.
- `apps/nmp-app-podcast/src/store/podcast_keys.rs` uses in-memory keys and
  placeholder FNV-style public-key derivation.
- `apps/nmp-app-podcast/src/host_op_publish.rs` builds unsigned diagnostic
  JSON with `id: null` and `sig: null`.
- `apps/nmp-app-podcast/src/host_op_publish.rs` returns `relay_pending`; no
  signed event is published to relays.
- `podcast.discover_nostr` currently uses an HTTP gateway search path, not a
  first-class relay subscription path.
- Pure-Nostr subscription to a show/episode set is incomplete; discovery still
  leans on `feed` when present and RSS subscribe for the durable path.

## P0 Implementation Plan

1. Rename the typed raw views away from `NIP74Show`/`NIP74Episode`.
   Use `NipF4Show` and `NipF4Episode` consistently across parse/build/tests.
2. Correct show parsing/building.
   Remove required `d`; coordinate from event pubkey only; read/write
   `description`; keep `title`, `image`, `language`, category tags, and
   optional feed URL if supported.
3. Correct episode parsing/building.
   Remove required `d`; remove `a`; remove `published_at`; read/write
   `description`; read/write `audio`; identify episodes by event id under the
   podcast pubkey.
4. Update tests to fail on NIP-74 tags.
   Add negative assertions that `d`, `a`, `summary`, `published_at`, and
   `imeta` are absent from NIP-F4 show/episode output.
5. Replace placeholder per-podcast key derivation.
   Use the same secp256k1/signing stack as the rest of NMP. Persist each
   podcast private key in the proper Keychain/native secure store slot, not
   only in memory.
6. Sign events in Rust.
   Build full Nostr events with valid `id`, `pubkey`, `sig`, `created_at`,
   content, and tags before returning success.
7. Publish events to relays.
   Use the configured relay list/NIP-65 write relays. `relay_pending` may only
   remain as an intermediate queue state if a durable queue exists.
8. Publish and maintain author claims.
   After create/update/delete of owned podcasts, publish kind `10064` from the
   agent key with the current set of podcast pubkeys.
9. Implement relay-backed discovery and episode fetch.
   Query kind `10154` for shows and kind `54` by podcast pubkey for episodes.
   HTTP gateway search can remain a convenience wrapper, but the canonical
   path must be relay/substrate-backed.
10. Update iOS/Android UI semantics.
   Owned-podcast UI must not tell users a show/episode is published until a
   signed event has been accepted or queued durably for relay publish.
11. Add migration handling for existing NIP-74-derived local data.
   Existing `nostr_coordinate` and `owner_pubkey_hex` rows must either be
   migrated to NIP-F4 coordinates or explicitly treated as legacy read-only
   entries.
12. Validate against relays.
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
