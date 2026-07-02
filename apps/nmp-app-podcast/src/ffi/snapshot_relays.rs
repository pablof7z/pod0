//! Configured-app-relay projection helper for
//! [`super::snapshot::build_podcast_update`].
//!
//! Relay state is kernel-owned (NMP v0.2.1): the actor is the sole writer of
//! the [`nmp_core::AppRelaySlot`] reached via `NmpApp::configured_relays_handle`.
//! This helper takes a quick read snapshot of that slot and maps each
//! [`nmp_core::kernel::AppRelay`] (`url()` / `role()`) into the FFI
//! [`AppRelayRow`] the Swift App Relays editor reads. It never touches
//! `PodcastStore` — there is no relay state there.

use nmp_native_runtime::NmpApp;

use super::snapshot_update::AppRelayRow;

/// Project the kernel's configured relays into the snapshot row shape.
///
/// Reads the shared [`nmp_core::AppRelaySlot`] under a short-lived lock and
/// iterates via `as_slice()` so the inner `Vec` is never handed across a
/// boundary. A poisoned lock degrades to an empty list (D6).
///
/// # Safety
/// `app` must be the valid, live `*mut NmpApp` held by the `PodcastHandle`
/// (the same pointer the host-op handler dereferences). The actor thread is
/// joined before `nmp_app_free`, so the pointer is live for every projection
/// call.
pub(super) unsafe fn build_configured_relays(app: *mut NmpApp) -> Vec<AppRelayRow> {
    let slot = unsafe { &*app }.configured_relays_handle();
    let Ok(guard) = slot.lock() else {
        return Vec::new();
    };
    guard
        .as_slice()
        .iter()
        .map(|r| AppRelayRow {
            url: r.url().to_string(),
            role: r.role().to_string(),
        })
        .collect()
}
