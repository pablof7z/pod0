//! ADR-0064 typed dispatch — the `nmp_app_podcast_dispatch_action` C symbol.
//!
//! Replaces the deleted `nmp_app_dispatch_action(app, namespace, json)` JSON
//! doorway from nmp-ffi ≤ v0.7.2. Takes a [`PodcastHandle`] pointer (from
//! `nmp_app_podcast_register`) rather than the raw `NmpApp` pointer — the
//! handle carries the underlying `app` pointer internally.
//!
//! Return envelope format is identical to the retired symbol so Swift callers
//! (and `DispatchResult.parse`) need no changes:
//! - accept → `{"correlation_id":"podcast-N"}`
//! - reject → `{"error":"<reason>"}`

use std::ffi::{c_char, CStr, CString};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

/// Dispatch a namespace-keyed action through the typed byte doorway.
///
/// Replaces the deleted `nmp_app_dispatch_action` JSON doorway from nmp-ffi
/// ≤ v0.7.2. Takes the `PodcastHandle` pointer returned by
/// `nmp_app_podcast_register` (which carries the underlying `NmpApp`).
///
/// Returns a `malloc`-allocated JSON string on accept
/// (`{"correlation_id":"podcast-N"}`) or on rejection (`{"error":"..."}`).
/// Returns NULL only when `handle`, `namespace`, or `action_json` is NULL
/// (D6: never crashes on a non-NULL handle). Caller MUST free via
/// `nmp_free_string`.
///
/// # Safety
/// - `handle` must be a valid non-null pointer from `nmp_app_podcast_register`
///   and must not have been freed yet.
/// - `namespace` and `action_json` must be valid nul-terminated UTF-8 C strings.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_dispatch_action(
    handle: *mut PodcastHandle,
    namespace: *const c_char,
    action_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || namespace.is_null() || action_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_dispatch_action", std::ptr::null_mut, || {
        let ns = match unsafe { CStr::from_ptr(namespace) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let json = match unsafe { CStr::from_ptr(action_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        // SAFETY: handle is non-null (checked above); the caller contract
        // guarantees it came from `nmp_app_podcast_register` and has not yet
        // been freed.
        let app = unsafe { (*handle).app };
        let envelope = match crate::dispatch_bytes::dispatch_action_bytes_for(app, ns, json) {
            Ok(correlation_id) => format!(r#"{{"correlation_id":"{correlation_id}"}}"#),
            Err(e) => {
                let escaped = e.replace('\\', r"\\").replace('"', r#"\""#);
                format!(r#"{{"error":"{escaped}"}}"#)
            }
        };
        CString::new(envelope)
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut())
    })
}
