//! Persist the kernel's configured-relay list to the C-ABI relay-config
//! sidecar after a relay edit.
//!
//! This is the save half of the C-ABI relay persistence; the load half lives
//! in [`super::data_dir`] and the on-disk format in
//! [`crate::store::relay_config`]. It lives in the FFI layer (not the store)
//! because reading the relay list requires the raw `*mut NmpApp` handle and the
//! kernel's `AppRelaySlot` — FFI concerns the store layer must not carry.

use std::sync::Mutex;

use nmp_ffi::NmpApp;

use crate::store::relay_config::save_relay_config;
use crate::store::PodcastStore;

/// Mirror the kernel's current configured relays into the
/// `.nmp-relay-config.json` sidecar under the store's bound data dir.
///
/// Reads the `AppRelaySlot` — the source of truth, already mutated by the
/// FIFO-ordered `ActorCommand::AddRelay`/`RemoveRelay` that ran before the
/// host-op calling this — and projects each `(url, role)` exactly as the
/// snapshot does (`url()` / `role().to_string()`), so the persisted list is
/// byte-identical to what iOS sees.
///
/// Degrades silently (D6) on a missing data dir (store not yet bound), a null
/// `app`, or a poisoned lock. A write failure is logged but non-fatal — the
/// in-memory edit still took effect for the session.
///
/// # Safety contract
/// `app` must be the live `*mut NmpApp` the host-op handler was constructed
/// with. The actor thread is joined before `nmp_app_free`, so the pointer is
/// valid for the duration of any host-op dispatch (a null pointer is handled).
pub(crate) fn persist_configured_relays(app: *mut NmpApp, store: &Mutex<PodcastStore>) {
    if app.is_null() {
        return;
    }
    let Some(data_dir) = store
        .lock()
        .ok()
        .and_then(|s| s.data_dir().map(std::path::Path::to_path_buf))
    else {
        return;
    };
    // SAFETY: non-null per the guard above; live per the contract documented
    // on this function.
    let slot = unsafe { &*app }.configured_relays_handle();
    let Ok(guard) = slot.lock() else {
        return;
    };
    let relays: Vec<(String, String)> = guard
        .as_slice()
        .iter()
        .map(|r| (r.url().to_string(), r.role().to_string()))
        .collect();
    drop(guard);
    if let Err(e) = save_relay_config(&data_dir, &relays) {
        eprintln!("[podcast] relay-config persist failed: {e}");
    }
}
