//! Per-domain typed projection builders and registration helpers.
//!
//! Each domain owns:
//!  - A serializable payload type (JSON, wrapped in `TypedProjectionData`)
//!  - A `build_<domain>_domain_payload` helper that assembles the payload from the handle
//!  - A `register_<domain>_projection` free function that wires the typed projection
//!    closure into the app via `register_typed_snapshot_projection`
//!
//! ## Delta semantics
//!
//! Each closure maintains a `last_emitted: AtomicU64`. On every tick it reads the
//! domain's `Arc<AtomicU64>` rev:
//!  - If domain rev == last_emitted → return `None` (sidecar omitted from the frame)
//!  - If domain rev > last_emitted  → serialize the payload, update last_emitted, return `Some`
//!
//! This gives true per-domain delta: a playback tick that bumps only `domain_revs.playback`
//! results in a frame carrying only the `podcast.playback` sidecar — the `podcast.library`
//! closure sees no change and returns `None`, so the library sidecar is absent from the frame.
//!
//! ## Tombstone contract
//!
//! Domains whose payload builder returns `Option<Value>` (library, downloads, identity, widget)
//! must NOT silently drop the sidecar when the domain rev ADVANCED but the state is now empty.
//! Dropping it would:
//!  1. Leave shell consumers unable to detect "cleared" state (sign-out, all-unsubscribed, etc.).
//!  2. Keep `last_emitted < domain_rev` forever, causing a full `build_podcast_update` + store
//!     lock on EVERY subsequent tick (perpetual rebuild).
//!
//! When `domain_rev != last_emitted` AND the payload builder yields `None`, the closure emits a
//! **tombstone** sidecar — a minimal payload carrying `"rev"` and the domain's primary field set
//! to `null` — and advances `last_emitted` to the current domain rev.  After the tombstone the
//! domain idles (last_emitted caught up) until its rev is bumped again.
//!
//! Per-domain tombstone shapes (the `null` field is the unambiguous "empty" signal):
//!  - `podcast.library`:   `{"rev": <u64>, "library": null}`
//!  - `podcast.downloads`: `{"rev": <u64>, "downloads": null}`
//!  - `podcast.identity`:  `{"rev": <u64>, "active_account": null}`
//!  - `podcast.widget`:    `{"rev": <u64>, "widget": null}`
//!
//! Domains whose builder is infallible (`podcast.playback`, `podcast.settings`,
//! `podcast.misc`) never emit tombstones — they always carry a full payload.
//!
//! ## Full emit table
//!
//! | State                          | Sidecar emitted?          |
//! |--------------------------------|---------------------------|
//! | `domain_rev == last_emitted`   | `None` (omitted)          |
//! | `domain_rev != last_emitted`, payload non-empty | full payload |
//! | `domain_rev != last_emitted`, payload empty/None | tombstone    |
//!
//! ## Decoder contract
//!
//! The payload is a JSON-encoded byte vector wrapped in `TypedProjectionData`.
//! `schema_id` is the projection key (e.g. `"podcast.library"`).
//! `nmp_app_podcast_decode_update_frame` decodes all `podcast.*` sidecars and
//! injects them under `v.projections[key]` in the bridge JSON, so iOS/Android shells
//! can consume them without waiting for the pull path.
//!
//! ## CodingKeys contract (from MEMORY: FFI decode snake_case contract)
//!
//! Types embedded in the domain payloads MUST NOT carry explicit snake_case `CodingKeys`
//! — the bridge decoder uses `convertFromSnakeCase`; explicit CodingKeys override it and
//! cause `keyNotFound` errors that drop the entire frame. All field names must use Rust
//! snake_case and rely on the bridge's automatic conversion.
//!
//! ## Module layout
//!
//! Payload builders live in the sibling `snapshot_domain_builders` module to keep
//! this file within the 500-line hard limit (AGENTS.md).  Store helpers used by
//! those builders live in `snapshot_domain_store_helpers`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nmp_core::TypedProjectionData;

use super::handle::PodcastHandle;
use super::snapshot_domain_builders::{
    build_downloads_payload, build_identity_payload, build_library_payload, build_misc_payload,
    build_playback_payload, build_settings_payload, build_social_payload, build_widget_payload,
};

// ── Schema IDs ────────────────────────────────────────────────────────────────

pub const SCHEMA_LIBRARY: &str = "podcast.library";
pub const SCHEMA_PLAYBACK: &str = "podcast.playback";
pub const SCHEMA_DOWNLOADS: &str = "podcast.downloads";
pub const SCHEMA_SETTINGS: &str = "podcast.settings";
pub const SCHEMA_IDENTITY: &str = "podcast.identity";
pub const SCHEMA_WIDGET: &str = "podcast.widget";
pub const SCHEMA_SOCIAL: &str = "podcast.social";
pub const SCHEMA_MISC: &str = "podcast.misc";

// ── Tombstone builders ────────────────────────────────────────────────────────

/// Tombstone for `podcast.library`: signals that the library is now empty
/// (all-unsubscribed). The `library` field is `null`.
fn library_tombstone(rev: u64) -> serde_json::Value {
    serde_json::json!({ "rev": rev, "library": null })
}

/// Tombstone for `podcast.downloads`: signals that all downloads cleared.
/// The `downloads` field is `null`.
fn downloads_tombstone(rev: u64) -> serde_json::Value {
    serde_json::json!({ "rev": rev, "downloads": null })
}

/// Tombstone for `podcast.identity`: signals sign-out / no active account.
/// The `active_account` field is `null`.
fn identity_tombstone(rev: u64) -> serde_json::Value {
    serde_json::json!({ "rev": rev, "active_account": null })
}

/// Tombstone for `podcast.widget`: signals that the widget has nothing to show.
/// The `widget` field is `null`.
fn widget_tombstone(rev: u64) -> serde_json::Value {
    serde_json::json!({ "rev": rev, "widget": null })
}

/// Tombstone for `podcast.social`: signals that all social state was cleared
/// (e.g. account switch). `social` field is `null` — the unambiguous "empty"
/// signal for the iOS/Android social domain frame consumer.
fn social_tombstone(rev: u64) -> serde_json::Value {
    serde_json::json!({ "rev": rev, "social": null })
}

// ── TypedProjectionData assembly ──────────────────────────────────────────────

fn make_typed(schema_id: &str, payload: serde_json::Value) -> TypedProjectionData {
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    TypedProjectionData {
        key: schema_id.to_string(),
        schema_id: schema_id.to_string(),
        schema_version: 1,
        file_identifier: String::new(),
        payload: bytes,
    }
}

// ── Registration helpers ──────────────────────────────────────────────────────

/// Register all per-domain typed snapshot projections.
///
/// Each closure captures:
///  - `handle: Arc<PodcastHandle>` — for state access
///  - `domain_rev: Arc<AtomicU64>` — the domain's own rev counter
///  - `last_emitted: Arc<AtomicU64>` — tracks what was last emitted; starts at 0
///
/// On every actor tick the closure:
///  1. Reads domain rev.
///  2. If unchanged since last_emitted → return `None` (sidecar omitted).
///  3. If changed AND payload non-empty → emit full payload, advance last_emitted.
///  4. If changed AND payload empty/None → emit tombstone, advance last_emitted.
pub fn register_domain_projections(
    app_ref: &nmp_ffi::NmpApp,
    handle: &Arc<PodcastHandle>,
) {
    let domain_revs = Arc::clone(&handle.state.infra.domain_revs);

    // ── podcast.library ───────────────────────────────────────────────────────
    {
        let h = Arc::clone(handle);
        let domain_rev = Arc::clone(&domain_revs.library);
        let last_emitted = Arc::new(AtomicU64::new(0));
        app_ref.register_typed_snapshot_projection(SCHEMA_LIBRARY, move || {
            let current = domain_rev.load(Ordering::Relaxed);
            let prev = last_emitted.load(Ordering::Relaxed);
            if current == prev {
                return None;
            }
            let payload = build_library_payload(&h)
                .unwrap_or_else(|| library_tombstone(current));
            last_emitted.store(current, Ordering::Relaxed);
            Some(make_typed(SCHEMA_LIBRARY, payload))
        });
    }

    // ── podcast.playback ──────────────────────────────────────────────────────
    {
        let h = Arc::clone(handle);
        let domain_rev = Arc::clone(&domain_revs.playback);
        let last_emitted = Arc::new(AtomicU64::new(0));
        app_ref.register_typed_snapshot_projection(SCHEMA_PLAYBACK, move || {
            let current = domain_rev.load(Ordering::Relaxed);
            let prev = last_emitted.load(Ordering::Relaxed);
            if current == prev {
                return None;
            }
            let payload = build_playback_payload(&h);
            last_emitted.store(current, Ordering::Relaxed);
            Some(make_typed(SCHEMA_PLAYBACK, payload))
        });
    }

    // ── podcast.downloads ─────────────────────────────────────────────────────
    {
        let h = Arc::clone(handle);
        let domain_rev = Arc::clone(&domain_revs.downloads);
        let last_emitted = Arc::new(AtomicU64::new(0));
        app_ref.register_typed_snapshot_projection(SCHEMA_DOWNLOADS, move || {
            let current = domain_rev.load(Ordering::Relaxed);
            let prev = last_emitted.load(Ordering::Relaxed);
            if current == prev {
                return None;
            }
            let payload = build_downloads_payload(&h)
                .unwrap_or_else(|| downloads_tombstone(current));
            last_emitted.store(current, Ordering::Relaxed);
            Some(make_typed(SCHEMA_DOWNLOADS, payload))
        });
    }

    // ── podcast.settings ──────────────────────────────────────────────────────
    {
        let h = Arc::clone(handle);
        let domain_rev = Arc::clone(&domain_revs.settings);
        let last_emitted = Arc::new(AtomicU64::new(0));
        app_ref.register_typed_snapshot_projection(SCHEMA_SETTINGS, move || {
            let current = domain_rev.load(Ordering::Relaxed);
            let prev = last_emitted.load(Ordering::Relaxed);
            if current == prev {
                return None;
            }
            let payload = build_settings_payload(&h);
            last_emitted.store(current, Ordering::Relaxed);
            Some(make_typed(SCHEMA_SETTINGS, payload))
        });
    }

    // ── podcast.identity ──────────────────────────────────────────────────────
    //
    // Dual-gated: the app-owned `domain_revs.identity` counter AND the kernel's
    // authoritative active-account slot (V-82 single source of truth). The
    // local-key path (`ImportNsec`/`Generate`/`Clear`) advances the rev counter;
    // an external NIP-55 / Amber sign-in lands the account by writing the kernel
    // slot from inside the kernel (`set_accounts` after `AddSigner { make_active:
    // true }`) and never touches the app's `IdentityStore`, so it does NOT
    // advance the rev counter. Gating on the rev alone omitted the identity
    // sidecar from the very frame the kernel emitted for the account-change,
    // leaving the host on "Not signed in" after a successful Amber sign-in (PR
    // #417 propagation defect — flaky because an unrelated identity mutation
    // occasionally bumped the rev and dragged the fresh slot along). Observing
    // the slot value here closes the gap with no polling: the closure only
    // samples the slot when it already runs on a kernel-driven emit, and emits
    // when EITHER the rev advanced OR the active-account hex changed.
    {
        let h = Arc::clone(handle);
        let domain_rev = Arc::clone(&domain_revs.identity);
        let last_emitted = Arc::new(AtomicU64::new(0));
        let last_active_hex: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        app_ref.register_typed_snapshot_projection(SCHEMA_IDENTITY, move || {
            let current = domain_rev.load(Ordering::Relaxed);
            let prev = last_emitted.load(Ordering::Relaxed);
            let kernel_hex = super::snapshot_identity::kernel_active_account_hex(&h);
            let rev_changed = current != prev;
            // Compare-and-record the kernel active-account hex under the lock so
            // the "changed?" decision and the bookkeeping update are atomic. D6:
            // a poisoned lock degrades to "rev gate only", never a panic across
            // the FFI boundary.
            let kernel_changed = match last_active_hex.lock() {
                Ok(mut guard) => {
                    let changed = *guard != kernel_hex;
                    if changed {
                        *guard = kernel_hex.clone();
                    }
                    changed
                }
                Err(_) => false,
            };
            if !rev_changed && !kernel_changed {
                return None;
            }
            let payload = build_identity_payload(&h)
                .unwrap_or_else(|| identity_tombstone(current));
            last_emitted.store(current, Ordering::Relaxed);
            Some(make_typed(SCHEMA_IDENTITY, payload))
        });
    }

    // ── podcast.widget ────────────────────────────────────────────────────────
    {
        let h = Arc::clone(handle);
        let domain_rev = Arc::clone(&domain_revs.widget);
        let last_emitted = Arc::new(AtomicU64::new(0));
        app_ref.register_typed_snapshot_projection(SCHEMA_WIDGET, move || {
            let current = domain_rev.load(Ordering::Relaxed);
            let prev = last_emitted.load(Ordering::Relaxed);
            if current == prev {
                return None;
            }
            let payload = build_widget_payload(&h)
                .unwrap_or_else(|| widget_tombstone(current));
            last_emitted.store(current, Ordering::Relaxed);
            Some(make_typed(SCHEMA_WIDGET, payload))
        });
    }

    // ── podcast.social ────────────────────────────────────────────────────────
    {
        let h = Arc::clone(handle);
        let domain_rev = Arc::clone(&domain_revs.social);
        let last_emitted = Arc::new(AtomicU64::new(0));
        app_ref.register_typed_snapshot_projection(SCHEMA_SOCIAL, move || {
            let current = domain_rev.load(Ordering::Relaxed);
            let prev = last_emitted.load(Ordering::Relaxed);
            if current == prev {
                return None;
            }
            let payload = build_social_payload(&h)
                .unwrap_or_else(|| social_tombstone(current));
            last_emitted.store(current, Ordering::Relaxed);
            Some(make_typed(SCHEMA_SOCIAL, payload))
        });
    }

    // ── podcast.misc ──────────────────────────────────────────────────────────
    {
        let h = Arc::clone(handle);
        let domain_rev = Arc::clone(&domain_revs.misc);
        let last_emitted = Arc::new(AtomicU64::new(0));
        app_ref.register_typed_snapshot_projection(SCHEMA_MISC, move || {
            let current = domain_rev.load(Ordering::Relaxed);
            let prev = last_emitted.load(Ordering::Relaxed);
            if current == prev {
                return None;
            }
            let payload = build_misc_payload(&h);
            last_emitted.store(current, Ordering::Relaxed);
            Some(make_typed(SCHEMA_MISC, payload))
        });
    }
}

// ── Decoder helper ────────────────────────────────────────────────────────────

/// Decode all `podcast.*` typed sidecars from a raw update-frame slice and
/// return them as a JSON object mapping `key → decoded_value`.
///
/// Returns `None` when no `podcast.*` sidecar is present (D6 — degrade
/// silently, never panic). A sidecar whose payload is not valid JSON is
/// silently skipped (D6).
pub fn decode_podcast_domain_sidecars(
    slice: &[u8],
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let typed = nmp_core::decode_snapshot_typed_projections(slice).ok()?;
    let mut map = serde_json::Map::new();
    for entry in typed {
        if entry.schema_id.starts_with("podcast.") {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&entry.payload) {
                map.insert(entry.schema_id, value);
            }
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "snapshot_domain_projection_tests.rs"]
mod tests;

/// Byte-identity regression guard for the slice-local playback queue rows —
/// split into its own file to keep both test files under the 500-line limit.
#[cfg(test)]
#[path = "snapshot_domain_queue_identity_tests.rs"]
mod queue_identity_tests;
