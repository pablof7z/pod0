# NMP v0.2.0 Upgrade Changelog

**Old revision/tag:** `ec15edef6e7012b96132b4128e691f0e17837438` (`nmp-v0.1.0`)
**New revision/tag:** `ae7b00481056a66894bec55c0817eeb9fb7b17a9` (`nmp-v0.2.0`, workspace version `0.2.0`)
**Commit count:** 156 commits since v0.1.0.
**Nature of change:** Pure dependency version bump. **C-ABI is byte-for-byte identical to v0.1.0 — no symbol migration required.** FlatBuffers pin is unchanged (`25.12.19` / `25.2.10`).

---

## TLDR

Non-breaking pin bump of the four git-pinned NMP workspace dependencies
(`nmp-app-template`, `nmp-core`, `nmp-ffi`, `nmp-signer-broker`) from `0.1.0` to
`0.2.0`. No podcast code changed. Everything new in v0.2.0 is either internal to
NMP, lands on Chirp/desktop/Android shells, or is additive Rust API / new
projection keys that podcast does not yet read. Nothing in this upgrade requires
adoption to keep podcast working.

### Verification

| Layer | Command | Result |
|---|---|---|
| 1 — workspace compile | `cargo check --workspace` | Pass (1m58s). All NMP crates resolve at `0.2.0` / rev `ae7b004`. One pre-existing local dead-code warning (`CompileOutcome` in `ai_chapters.rs`) unrelated to NMP. |
| 2 — iOS-sim build | `cargo build -p nmp-app-podcast --target aarch64-apple-ios-sim` | Pass (1m04s). No linker errors; same pre-existing warning only. |

The version-at-rev was verified before the bump: the workspace version is `"0.2.0"`
at the pinned commit `ae7b004` (the version bump landed before the tag's CHANGELOG
release commit `6c82436`).

---

## What's New in v0.2.0 (podcast-relevant)

Each item is marked **free** (benefits podcast with no adoption work) or
**optional adoption** (a new capability podcast could opt into later).

### Projections

- **`resolved_profiles` projection** (#812) — **optional adoption.** The kernel now
  pre-merges `claimed_profiles` + `author_view.profile` + `mention_profiles` into a
  single `projections["resolved_profiles"]` map on every snapshot tick. Other shells
  (iOS Chirp, Android, TUI, desktop, gallery) deleted their hand-rolled merge loops
  and read this key directly. Podcast does not currently merge profiles in the shell,
  so there is nothing to delete; this is available if/when podcast needs resolved
  author profiles. **Not adopting in this PR.**

- **`bunker_connection_state` projection** (#864, V-14) — **optional adoption.**
  `projections["bunker_connection_state"]` carries NIP-46 bunker session health
  (`state` = `connected`/`reconnecting`/`failed`, plus `is_connected`,
  `is_reconnecting`, `is_failed`, `reason`). The JSON key is present in every snapshot
  at this rev and readable via the raw projection dictionary today; typed iOS/Android
  decoding is forthcoming upstream. Podcast does not decode it yet. **Not adopting in
  this PR.**

- **`claimed_events` / `claimed_profiles` projections** (#795/#803) — **free.**
  Component-owned events/profiles surfaced via `projections`. `TimelineItem` gains
  `authorDisplayName` (#823). Podcast does not use the generic timeline; no impact.

### Performance

- **O(1) snapshot hot path** (#873) — **free.** `estimated_store_bytes` changed from
  O(store) to O(1), eliminating a twice-per-emit linear scan that serialized inside
  the snapshot path. Every podcast snapshot tick is cheaper at no cost.

### Fixes podcast benefits from automatically

- **D1 startup ordering** (#835, V-87) — **free.** The first kernel snapshot no longer
  depends on relay I/O; the shell receives an initial snapshot immediately on launch
  even when offline. (Hardens the same D1 invariant tightened in v0.1.0.)

- **Actor-thread unfreeze — V-90 Sites 1 + 2** (#861 / #870) — **free.** Two
  synchronous capability calls that ran on the actor thread and blocked all kernel
  processing were moved off-actor via the capability-worker seam (ADR-0040):
  Site 1 is the NIP-17 gift-wrap `op.wait()` during NIP-46 remote-signer round trips
  (`nmp-nip17`); Site 2 is a synchronous OS-keychain dispatch. Any podcast flow that
  triggers a bunker round trip or keychain access no longer stalls the kernel. This
  is the upstream-canonical fix for the actor-thread-blocking class of bug.

- **NWC heartbeat + reconnect** (#783, V-79) — **free.** `nmp-nip47` now reconnects on
  NWC connection drop and emits a `connection_state` projection (previously silent).
  Relevant if/when podcast wires Nostr Wallet Connect; no action needed now.

### Testing / env

- **`NMP_MARMOT_MOCK_KEYRING` env var** (#872) — **free (test-only).** Set to
  `1`/`true`/`yes`/`on` to route MLS (Marmot) key storage through an in-memory mock
  instead of the OS keychain, enabling headless CI of MLS group flows. Podcast does
  not use Marmot; available if needed for CI.

### Other upstream work (no podcast surface)

Android UI screens (DM/wallet/profile/relay/sign-in/zap), chirp-desktop feature
additions, `nmp-kinds` Layer-0 crate (#857), `NmpAppBuilder` typestate (#858),
registry system (#787/#819), V-42 NIP-51 mute list, V-52 single-relay browsing,
V-60 LRU store eviction, V-68 `{1,6}` kind → FFI-shim layering refactor, and the
typed Rust client API for Chirp — all internal to NMP or other shells, all **free**
(compile clean as transitive deps, no podcast behavior change).

---

## What we are NOT adopting yet

- **`resolved_profiles` migration.** Podcast does not merge author profiles in the
  shell, so there is no merge loop to retire. Defer until podcast needs resolved
  profile rendering.
- **`bunker_connection_state` decode.** The projection key ships in every snapshot,
  but podcast has no typed decoder or UI for bunker session health. Defer until there
  is a user-facing surface for NIP-46 connection state.

Both are tracked as future opt-ins; neither blocks this bump.

---

## Deprecations (informational — no podcast impact)

- **`nmp_marmot_snapshot` / `nmp_marmot_group_messages`** (pull-model Marmot C-ABI
  symbols) are deprecated per ADR-0039 in favor of the push-projection seam. They
  remain functional. Podcast does not link Marmot symbols, so no action required.
