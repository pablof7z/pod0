//! `nmp_app_podcast_http_report` — async platform→Rust HTTP-report channel for
//! the optimistic-subscribe feed fetch.
//!
//! `handle_subscribe` inserts the podcast row optimistically and dispatches the
//! RSS fetch through the **async** HTTP capability
//! ([`podcast_feeds::http::HttpCommand`]). The platform executor (iOS
//! `HttpCapability`, Android, TUI) runs the transport off its main/actor thread
//! and, on completion, fires this entry point with the JSON-encoded
//! [`HttpReport`]. The kernel resolves the matching pending fetch through the
//! [`crate::feed_fetch::FeedFetchCoordinator`], parses the feed, merges
//! episodes, and bumps the snapshot rev so the hydrated episodes reach the
//! shell.
//!
//! Mirrors the download-report shim (`download_report.rs`): the executor pushes
//! the report via this FFI from its transport-completion callback, exactly how
//! `DownloadCapability` calls `nmp_app_podcast_download_report`.
//!
//! ## Wire protocol
//!
//! * **Request**: `report_json` is a JSON-encoded [`HttpReport`]
//!   (`{"request_id":"…","result":{"status":"ok",…}}`).
//! * **Response**: always `NULL` — unlike downloads there is no follow-up
//!   command for the platform to execute. The async hydration is entirely
//!   kernel-side; the projection delivers the result.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, and decode failures all return `NULL` and
//! never panic across the FFI. An unknown / already-resolved `request_id` is a
//! no-op inside the coordinator.

use std::ffi::{c_char, CStr};

use podcast_feeds::http::HttpReport;

use super::handle::PodcastHandle;

/// Deliver a JSON-encoded [`HttpReport`] to the kernel's feed-fetch
/// coordinator. Always returns `NULL` (no follow-up); nothing to free.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_http_report(
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

    let report: HttpReport = match serde_json::from_str(report_str) {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    // Runs on the platform transport thread — `apply_report` touches only the
    // shared store / signal Arcs, never `*mut NmpApp`.
    let handle_ref = unsafe { &*handle };
    handle_ref.feed_fetch.apply_report(report);
    std::ptr::null_mut()
}
