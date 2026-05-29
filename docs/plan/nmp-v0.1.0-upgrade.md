# NMP v0.1.0 Upgrade Changelog

**Old revision:** `2f06cc66a4126e6b71a9c5d818cdb3bfd286fd49`  
**New revision/tag:** `ec15edef6e7012b96132b4128e691f0e17837438` (`nmp-v0.1.0`)  
**Commit count:** 75 commits  
**Diff scope:** 432 files changed, ~38 058 insertions / ~7 778 deletions

---

## Executive Summary (5 most impactful changes for podcast)

1. **`cargo check --workspace` passes cleanly.** The Rust-side upgrade is a no-op break for podcast: all API changes are either additive or internal to Chirp/desktop. No podcast code required modification.
2. **V-46: View-dependent snapshot keys (`timeline`, `inserted`, `updated`, `removed`, `author_view`, `thread_view`) are now gated — absent until the view is open.** Podcast's Swift shell does not use any of these keys (all projection is in Rust via `PodcastHandle`), so no impact, but any new code touching those keys must treat them as optional.
3. **ADR-0037 — typed FlatBuffers projection sidecar (`typed_projections`) added to every snapshot frame.** New `nmp_core::TypedProjectionData`, new `encode_snapshot_with_typed` / `decode_snapshot_with_typed` transport functions, and `NmpApp::register_typed_snapshot_projection` host seam. Backward-compatible wire (empty sidecar = identical bytes). Podcast can adopt this for future feed projections.
4. **V-82/V-83 — `NmpApp::active_account_handle()` and `NmpApp::event_by_id()` expose kernel internals as shared slots.** Two new public accessors on `NmpApp`; the actor constructor signature expanded with `active_account_slot` and `event_store_slot` parameters. Podcast's `nmp_app_podcast_register` calls neither yet — no action required now, but `active_account_handle` is the new authoritative source of the signed-in pubkey.
5. **D1 fix — initial snapshot now emitted BEFORE relay connections are dialed** (Reset arm reorder). Podcast benefits silently: the shell now always sees a valid first frame before any network I/O.

---

## Breaking Changes

### (None in the Rust/C-ABI surface that affect podcast)

The following changes are **breaking within NMP** but have **zero impact on `nmp-app-podcast`** because:

- Podcast never links `nmp-app-chirp` or calls `nmp_app_chirp_snapshot` / `nmp_app_chirp_snapshot_free`.
- Podcast does not call `nmp_nip47::wallet_disconnect`, `wallet_pay_invoice`, or `handle_nwc_text` directly (they are now `pub(crate)` — but podcast already routed through action-module command shapes).
- Podcast does not call `nmp_nwc::make_invoice_content` / `MakeInvoiceParams` / `MakeInvoiceResult` (deleted).

The full list for completeness:

#### 1. `nmp_app_chirp_snapshot` and `nmp_app_chirp_snapshot_free` C-ABI symbols deleted (commit `242802d7`)
- **What changed:** The two deprecated C-ABI functions for the Chirp snapshot were removed from `nmp-app-chirp`. All Chirp callers migrated to `ChirpHandle::snapshot()` or typed FlatBuffers streams.
- **Podcast impact:** None. These symbols never existed in `NmpCore.h` or any podcast bridge file. Confirmed: grep of `App/Sources/Bridge/` shows no reference.

#### 2. `nmp_nip47`: `wallet_disconnect`, `wallet_pay_invoice`, `handle_nwc_text` visibility narrowed to `pub(crate)` (commit `b64c92f5`)
- **What changed:** Three internal runtime functions incorrectly exported as `pub` are now `pub(crate)`. Callers were already expected to use the `WalletConnectCommand` / `WalletDisconnectCommand` / `WalletPayInvoiceCommand` action shapes.
- **Podcast impact:** None. Podcast routes all wallet operations through the action/capability dispatch seam, not these low-level functions.

#### 3. `nmp_nwc::MakeInvoice` API deleted (commit `77d11bcc`)
- **What changed:** `make_invoice_content`, `MakeInvoiceParams`, and `MakeInvoiceResult` removed from `nmp-nwc`. The NWC NIP-47 `make_invoice` method was dead code.
- **Podcast impact:** None. Not used by podcast.

#### 4. `run_actor_with_observers` signature extended with two new trailing parameters (commits `344d7aa7`, `456c553f`)
- **What changed:** `run_actor_with_observers` now requires `active_account_slot: crate::slots::ActiveAccountSlot` and `event_store_slot: crate::slots::EventStoreSlot` as two additional trailing parameters. Every existing call site that does not go through `nmp_app_new` (the FFI entry) is affected.
- **Podcast impact:** Podcast does not call `run_actor_with_observers` directly — it goes through `nmp_app_new` → `nmp_app_podcast_register`. Zero impact. Internal NMP callers (`run_actor`, `run_actor_with_lifecycle_observer`) forward throwaway slots automatically.

---

## New Capabilities / Features

### ADR-0037 — Typed FlatBuffers sidecar in snapshot frames
**Commits:** `c0c8e6a5`, `fab66779`, `915a86a6` and subsequent  
**Crates affected:** `nmp-core`, `nmp-ffi`

Every `SnapshotFrame` now carries an optional `typed_projections` vector alongside the existing generic `Value` JSON. The transport is opaque — `nmp-core` never interprets the typed bytes.

New public API in `nmp-core`:
- `TypedProjectionData` struct: `key`, `schema_id`, `schema_version`, `file_identifier`, `payload: Vec<u8>`.
- `encode_snapshot_with_typed(snapshot: Value, typed: &[TypedProjectionData]) -> UpdateFrameBytes` — replaces `encode_snapshot_value` when sidecar entries exist.
- `decode_snapshot_with_typed(bytes: &[u8]) -> Result<(Value, Vec<TypedProjectionData>), UpdateFrameDecodeError>` — superset of `decode_snapshot_payload`.
- `encode_snapshot_value` still works and produces wire-identical bytes when no sidecar is present (backward-compatible).
- `SnapshotRegistry::register_typed(key, f: Fn() -> Option<TypedProjectionData>)` — registers a typed projection closure.
- `TypedProjectionFn` type alias exported from `__ffi_internal`.

New public API in `nmp-ffi`:
- `NmpApp::register_typed_snapshot_projection(key, f)` — the host seam for registering typed closures.

**Podcast impact:** No action required. The wire format is backward-compatible. This is the foundation for future typed feed projections (e.g. a typed `nmp.feed.podcast.home` sidecar). The podcast snapshot decode path (`KernelBridge+Decode.swift`) reads the generic Value tree and will continue to work unchanged.

---

### V-82 — `NmpApp::active_account_handle()` — single-source-of-truth active account slot
**Commit:** `344d7aa7`  
**Crates affected:** `nmp-ffi`, `nmp-core`

New public method on `NmpApp`:
```rust
pub fn active_account_handle(&self) -> ActiveAccountSlot
// where ActiveAccountSlot = Arc<Mutex<Option<String>>>
```

Returns the same `Arc` the kernel actor writes on every identity mutation (sign-in, account-switch, logout). The actor constructs the slot at `nmp_app_new` and hands it to the kernel at startup (and on `Reset`).

New exports:
- `nmp_core::slots::ActiveAccountSlot` (type alias)
- `nmp_core::slots::new_active_account_slot()` constructor
- `Kernel::with_storage_path_and_account_slot(visible_limit, storage_path, slot)` constructor

**Podcast impact:** No action required now. The nsec import path (`nmp_app_signin_nsec`) is unchanged in signature and semantics — `identity.rs` changes were formatting-only. Podcast currently reads the signed-in pubkey from the `accounts` / `active_account` snapshot keys (JSON projection). `active_account_handle` is the new lower-latency alternative for any future code that needs the pubkey synchronously between snapshot ticks, but switching is optional.

---

### V-83 — `NmpApp::event_by_id()` — synchronous event lookup via kernel store
**Commit:** `456c553f`  
**Crates affected:** `nmp-ffi`, `nmp-core`

New public methods/types on `NmpApp`:
```rust
pub fn event_store_handle(&self) -> EventStoreSlot
// where EventStoreSlot = Arc<Mutex<Option<Arc<dyn EventStore>>>>

pub fn event_by_id(&self, id: &str) -> Option<nmp_core::substrate::KernelEvent>
```

The actor publishes the kernel's `EventStore` handle into the slot after kernel construction (and on `Reset`). This enables synchronous event lookups from host code without waiting for a snapshot tick.

New exports:
- `nmp_core::slots::EventStoreSlot` (type alias)
- `nmp_core::slots::new_event_store_slot()` constructor
- `nmp_core::slots::event_by_id_from_store(slot, id)` standalone helper

**Podcast impact:** No action required. This enables future podcast features like repost/reply backward-hydration. Not currently used by podcast.

---

### V-80 — OP-centric home feed composition root in `nmp-app-template`
**Commits:** Multiple (V-80 rungs 1–7, spanning `4f764470`..`105723a1`, `a54f8bfc`, `0b140af8`)  
**Crates affected:** `nmp-app-template`, `nmp-feed`, `nmp-nip01`, `nmp-nip02`

`nmp-app-template` now depends on `nmp-ffi`, `nmp-nip01`, and `nmp-feed`. A new public module:
```rust
pub mod op_feed_defaults;
pub use op_feed_defaults::{register_op_feed_defaults, OpFeedDefaults};
```

`register_op_feed_defaults` wires the full OP-centric home-feed composition: `ActiveFollowSet`, `OpFeedEngine`, follow-predicate, event-lookup closure, claim sink, and `FeedController` registered under `"nmp.feed.home"`. The Chirp app has already migrated; this function is available to podcast for future feed work.

New crates introduced as `nmp-app-template` dependencies: `nmp-nip01`, `nmp-feed`.

**Podcast impact:** No action required. `nmp_app_template::register_defaults` (which podcast calls in `nmp_app_podcast_register`) still works unchanged — it does NOT call `register_op_feed_defaults` (that is separate, opt-in, Chirp-specific). The new dependencies add to the compile graph but do not affect podcast behavior.

---

### `SignerError::KindOutOfRange` — new error variant in signer interface
**Commit:** `6d14d622`  
**Crate affected:** `nmp-signer-iface`

```rust
pub enum SignerError {
    // ... existing variants ...
    KindOutOfRange { kind: u32 },   // NEW
    Backend(String),
}
```

`LocalKeySigner::sign_now` now returns this error instead of silently coercing an out-of-range kind to `u16::MAX`.

**Podcast impact:** Additive variant. Podcast kinds are all standard (0, 3, 1, 1111, NIP-F4 range) — all fit comfortably in `u16`. Any exhaustive `match` on `SignerError` in podcast crates must add a `KindOutOfRange` arm; `cargo check` confirms none exist (clean build). Stricter validation is strictly safer for podcast publishing.

---

### `store_open_failure` — new snapshot field (V-67)
**Commit:** `176c4418`  
**Crate affected:** `nmp-core`

A new optional field `store_open_failure: Option<String>` is added to `KernelSnapshot`. It is `None` in the healthy case and `Some(reason)` when a storage path was configured but the LMDB open failed (the kernel falls back to in-memory). The field is `#[serde(skip_serializing_if = "Option::is_none")]` so no wire change in healthy sessions.

The commit comment says "The host MUST surface this to the user (e.g. an alert or persistent banner)."

**Podcast impact:** The `KernelModel.swift` / snapshot decode path must be extended to surface this field. Currently no podcast code reads `store_open_failure`. This is a follow-up backlog item: decode `snapshot["store_open_failure"]` and present a user-facing alert if non-nil. The Swift `SnapshotProjections` (Generated types) will need a new optional field.

---

### `nmp-network` — reconnect backoff reset after healthy session (V-92)
**Commit:** `5da5942c`

The WebSocket relay client now resets its reconnect backoff after a sustained healthy session, so a relay that was previously flaky and triggered exponential backoff will reconnect at normal speed after recovering.

**Podcast impact:** Positive — relay reconnections after transient failures will recover faster. No API change.

---

### `nmp-nostr-lmdb` — surface orphan-index corruption as diagnostic (V-69)
**Commit:** `60085fc4`

Previously silent `ok()??` swallowed orphan-index corruption errors. Now surfaces them as diagnostics instead.

**Podcast impact:** Silent behavior → surfaced error. Podcast gets better observability on LMDB corruption. No API change.

---

### `nmp-marmot` — surface orphaned-commit and keyring-unavailable (V-61, V-62)
**Commit:** `d19bef78`

Previously silent failures in marmot operations now surface as diagnostics.

**Podcast impact:** Better observability. No API change.

---

## Behavior Changes (Silent — Review Carefully)

### D1 fix — initial snapshot emitted BEFORE relay TCP connections are dialed (commit `76a3cb0f`)
**Crate affected:** `nmp-core` (`actor/dispatch.rs`)

In the `Reset` dispatch arm, `emit_now` previously ran **after** `spawn_missing_relays`. This violated D1 ("first rendered frame must be independent of relay I/O"). The ordering is now corrected: `emit_now` runs first, then relay connections are dialed.

**Podcast impact:** The shell now reliably receives a valid first snapshot before any relay I/O occurs. This eliminates a race condition where the shell could attempt to render before receiving an initial frame. It is a correctness improvement with no API change.

---

### V-46 — view-dependent snapshot keys are now absent when no view is open (commit `2c0839b4`)
**Crate affected:** `nmp-core`

The keys `timeline`, `inserted`, `updated`, `removed`, `author_view`, and `thread_view` in `KernelSnapshot::projections` are now **conditionally present**:
- `timeline` / `inserted` / `updated` / `removed`: present only when `follow_feed_kinds` is non-empty (shell called `nmp_app_open_timeline`).
- `author_view`: present only when a view is open.
- `thread_view`: present only when a view is open.

Previously these six keys were always emitted (null/empty when no view was open).

**Podcast impact:** Confirmed safe. Podcast's `PodcastHandle` snapshot (`nmp_app_podcast_snapshot`) is entirely separate from the generic kernel snapshot and does not use any of these six keys. The Swift `AppStateStore+KernelProjection.swift` also does not reference them. No code change required. Any future podcast code reading the generic kernel snapshot must treat these as optional.

---

### V-68 — mailbox-change routing trace now uses empty kind slice instead of `{1, 6}` (commit `b24c95f2`)
**Crate affected:** `nmp-core` (`kernel/ingest/contacts.rs`)

The synthetic interest fired on NIP-65 relay list arrival (for routing-trace purposes) now passes `&[]` instead of `&[1, 6]`. Per the commit, the routing decision is kind-independent for read-lane routing; the change removes a hardcoded social default that violated D0.

**Podcast impact:** None. Podcast's subscription interests are set via host-declared `follow_feed_kinds` and podcast-specific kinds. Routing trace behavior is unchanged in practice.

---

### NWC — unknown URI params now surfaced + dead MakeInvoice API deleted (commit `77d11bcc`)
**Crate affected:** `nmp-nwc`

- Unknown NWC URI parameters are now surfaced as warnings instead of silently ignored.
- `make_invoice_content`, `MakeInvoiceParams`, `MakeInvoiceResult` deleted.

**Podcast impact:** NWC URI parse behavior is stricter. Podcast does not use `make_invoice_content`. No action required.

---

## New Crates Introduced

| Crate | Purpose | Podcast Impact |
|---|---|---|
| `nmp-nip01` | NIP-10 OP-centric feed engine (`OpFeedEngine`, `register_op_feed`) | Added as indirect dep via `nmp-app-template`; compiles clean |
| `nmp-nip18` | NIP-18 repost handling (wired into OP-feed repost hydration L-2/L-5) | Added as indirect dep; compiles clean |
| `nmp-content` | Typed FlatBuffers wire for `ContentTreeWire` (social post content graph) | Added as indirect dep; compiles clean |

These crates were present in the NMP repo but not compiled into the podcast target at the old pin. They now compile as indirect dependencies via `nmp-app-template` → `nmp-nip01` / `nmp-feed`.

---

## Deprecations / Removals

| Symbol | Type | Removed in | Podcast Impact |
|---|---|---|---|
| `nmp_app_chirp_snapshot` | `#[no_mangle]` C function | `242802d7` | None |
| `nmp_app_chirp_snapshot_free` | `#[no_mangle]` C function | `242802d7` | None |
| `nmp_nwc::make_invoice_content` | `pub fn` | `77d11bcc` | None |
| `nmp_nwc::MakeInvoiceParams` | `pub struct` | `77d11bcc` | None |
| `nmp_nwc::MakeInvoiceResult` | `pub struct` | `77d11bcc` | None |
| `nmp_nip47::wallet_disconnect` | narrowed to `pub(crate)` | `b64c92f5` | None |
| `nmp_nip47::wallet_pay_invoice` | narrowed to `pub(crate)` | `b64c92f5` | None |
| `nmp_nip47::handle_nwc_text` | narrowed to `pub(crate)` | `b64c92f5` | None |
| `nmp-desktop` crate (egui) | entire crate deleted | `77a671b9` | None (was Chirp-only) |
| `apps/android/podcast` | Android podcast app deleted | `0fae26ba` | None |
| `apps/longform`, `apps/notes` | deleted apps | `0fae26ba` | None |

---

## Compiler Breakages from cargo check

**Result: ZERO breakages. Both `cargo check --workspace` and `cargo check --workspace --all-targets` (includes all test targets) pass cleanly.**

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.54s  # lib targets
Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.97s   # all targets including tests
```

Three pre-existing unused-import warnings exist across the workspace — none are related to the NMP upgrade:
- `podcast-tui/src/app.rs:4`: `use crate::bridge::NmpEvent` (pre-existing)
- `apps/nmp-app-podcast/src/ffi/helpers_tests.rs:1`: `use super::*` (pre-existing)
- `apps/nmp-app-podcast/src/ffi/voice_report_tests.rs:1`: `use super::*` (pre-existing)

**Why no breakages:**
- All removed symbols (`nmp_app_chirp_snapshot`, `MakeInvoice*`, NIP-47 internal fns) were not used by any podcast crate.
- All new required parameters to `run_actor_with_observers` are injected automatically by `nmp_app_new` — podcast does not call this function directly.
- `SignerError::KindOutOfRange` is additive — no exhaustive match on `SignerError` in podcast code.
- V-46 view-key gating does not affect podcast's projection path.
- The Swift/C-ABI boundary is stable: `NmpCore.h` references no deleted symbols.

---

## Follow-up Backlog Items

1. **`store_open_failure` must be surfaced in the Swift shell.** Decode `snapshot["store_open_failure"]` in `KernelBridge+Decode.swift` / `KernelModel.swift` and show a user-facing alert or banner. Add `storeOpenFailure: String?` to the `SnapshotProjections` generated type.
2. **Consider `NmpApp::active_account_handle()` as canonical pubkey source.** Replace polling of the `active_account` snapshot key with a direct slot read where synchronous access to the pubkey is needed (e.g. NIP-F4 publishing flows).
3. **Typed FlatBuffers sidecar for podcast feed.** Once the podcast home feed is migrated to an `OpFeedEngine`-style projection, register a typed `nmp.feed.podcast.home` sidecar using `NmpApp::register_typed_snapshot_projection` and `decode_snapshot_with_typed` on the Swift side.
4. **Pre-existing TUI warning.** `podcast-tui/src/app.rs:4` has an unused import (`use crate::bridge::NmpEvent`) that `cargo fix` can remove. Low priority.
