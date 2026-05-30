//! `nmp_app_podcast_network_report` — iOS→Rust network-state report.
//!
//! The iOS `NetworkCapability` (backed by `NWPathMonitor`) fires this entry
//! point whenever the device's active network interface changes. Rust updates
//! the `PodcastStore.is_on_wifi` flag so that the next auto-download
//! evaluation honours the user's Wi-Fi-only preference correctly.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, lock poison, and decode failures all
//! return `NULL`. Nothing panics across the FFI.

use std::ffi::{c_char, CStr};

use super::handle::PodcastHandle;
use crate::capability::NetworkReport;

/// Deliver a JSON-encoded [`NetworkReport`] to the kernel. Returns `NULL` —
/// there is no synchronous follow-up command for network-state changes.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_network_report(
    handle: *mut PodcastHandle,
    report_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || report_json.is_null() {
        return std::ptr::null_mut();
    }

    let report_str = match unsafe { CStr::from_ptr(report_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let report: NetworkReport = match serde_json::from_str(report_str) {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    let handle_ref = unsafe { &*handle };
    match report {
        NetworkReport::ConnectivityChanged { is_wifi, .. } => {
            if let Ok(mut s) = handle_ref.store.lock() {
                s.set_is_on_wifi(is_wifi);
            }
            // When Wi-Fi is restored, pending deferred downloads are drained
            // and dispatched from `PodcastAction::DispatchDeferredWifiDownloads`.
            // The iOS NetworkCapability fires that action when `is_wifi` becomes
            // true so that the dispatch runs through the normal actor-thread path
            // (which has access to `PodcastHostOpHandler::dispatch_download`).
        }
    }

    std::ptr::null_mut()
}
