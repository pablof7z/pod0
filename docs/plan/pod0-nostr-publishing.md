# Plan: Pod0 NIP-F4 Nostr Publishing And Discovery

Canonical status: tracked from `docs/plan.md` and `docs/BACKLOG.md`.

## Current Status - 2026-06-14

NIP-F4 publishing is **substantially complete**. The following are done:
- Per-podcast secp256k1 key generation + file-backed persistence (`podcast-keys.json`, atomic write, reload on restart, cleanup on delete)
- Real event signing for kind:10154, kind:54, and kind:10064 (valid `id`/`pubkey`/`sig`)
- Relay publish via `dispatch_nostr_relay` → `wss://relay.primal.net` (returns `"published"` on relay acceptance)
- Blossom audio upload for episode events (with RSS enclosure fallback)
- Author claims (kind:10064) signed with agent key and published to relay
- kind:10154 show discovery via NMP relay pool (`NostrDiscoveryObserver` + `EnsureInterest`)
- feedless kind:54 episode fetch via NMP relay pool (`SubscribeNostr` +
  `push_interest_via_nmp` + `NostrEpisodesObserver`)

The remaining NIP-F4 work is:

The repo still has stale code names and old parser paths (`NIP74Show`,
`NIP74Episode`, `parse_episode_event`, and related comments/tests) alongside
the newer `NipF4Show` discovery parser and corrected publish builders. Treat
those as code debt until each path is either renamed/reworked for NIP-F4
or explicitly quarantined as a read-only compatibility path.

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
- `apps/nmp-app-podcast/src/nostr_episodes.rs` now subscribes to kind:54
  events by podcast pubkey through NMP's relay pool and upserts inbound
  feedless episodes into the shared podcast store.

## Remaining Work

- **Hardcoded relay URL.** `dispatch_nostr_relay` publishes to
  `wss://relay.primal.net` only. Should read the app's configured write relays.
  Currently a no-op because primal.net is the only configured write relay.
- **Stale type/parser names.** `NIP74Show`/`NIP74Episode` naming in
  `apps/podcast-discovery/` should be cleaned up or explicitly quarantined.
- **No durable retry queue.** If relay rejects an event, publish fails silently
  with `status: "signed"`. Acceptable for now; a retry queue is a follow-up.

## Next Steps

1. Fix `dispatch_nostr_relay` to read configured write relays instead of hardcoding primal.net.
2. Validate end-to-end on device: publish a show, confirm relay returns `"published"`, fetch it back.
3. Clean up `NIP74Show`/`NIP74Episode` naming in `podcast-discovery`.

## Done Criteria

- No active NIP-F4 publish/build test expects `d`, `a`, `summary`,
  `published_at`, or `imeta`.
- `KIND_SHOW == 10154`, `KIND_EPISODE == 54`, and
  `KIND_AUTHOR_CLAIM == 10064` are still pinned in one canonical module.
- Per-podcast keys survive restart and derive real secp256k1 pubkeys.
- `publish_show`, `publish_episode`, and `publish_author_claim` produce signed
  events and publish them to relays.
- Discovery can subscribe to a Nostr-only podcast without relying on RSS.
- Docs, tests, UI copy, and `whats-new.json` describe NIP-F4 as the
  canonical publish/discovery protocol.
