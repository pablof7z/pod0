//! `nmp_app_podcast_download_report` — async iOS→Rust download-report channel.
//!
//! The iOS `DownloadCapability` fires this FFI entry point whenever its
//! background `URLSession` delegate has a new `DownloadReport` to deliver
//! (`Completed`, `Cancelled`, …). Rust projects the report onto
//! [`crate::store::PodcastStore`] / [`crate::download::DownloadQueue`] and returns any follow-up
//! [`crate::capability::DownloadCommand`] the iOS side should execute.
//!
//! Mirrors the audio-report shim at `audio_report.rs` so the iOS bridge
//! ([`KernelBridge+Callbacks.swift::attachDownloadReportChannel`]) can use
//! the same return-and-execute pattern.
//!
//! ## Wire protocol
//!
//! * **Request**: `report_json` is a JSON-encoded
//!   [`crate::capability::DownloadReport`].
//! * **Response**: heap-allocated nul-terminated JSON of a
//!   [`crate::capability::DownloadCommand`], or `NULL` when no follow-up is
//!   needed. The caller MUST free the returned pointer via
//!   `nmp_app_free_string`. This is how the Rust queue starts the next
//!   waiting item after iOS reports `Completed`, `Failed`, or `Cancelled`.
//!
//! ## Lock discipline
//!
//! Acquires the `PodcastStore` and `DownloadQueue` locks, dispatches the
//! projection, drops both locks, then bumps `rev` so the next snapshot poll
//! sees the mutation. Matches the audio shim's discipline — never hold locks
//! across the rev bump, never panic across the FFI.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, lock poison, and decode failures all
//! return `NULL` (treated by iOS as "no follow-up command"). Nothing
//! panics across the FFI.

use std::ffi::{c_char, CStr, CString};

use super::handle::PodcastHandle;
use crate::capability::dispatch::dispatch_download_report_json_with_queue;
use crate::capability::dispatch::DispatchOutcome;

/// Deliver a JSON-encoded `DownloadReport` to the Rust `PodcastStore` and
/// return the JSON-encoded follow-up `DownloadCommand`, if any.
///
/// Returns a malloc-compatible string the caller MUST free via
/// `nmp_app_free_string`, or `NULL` when no follow-up is needed (or on any
/// error).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_download_report(
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

    let handle_ref = unsafe { &*handle };
    let follow_up_json = {
        let mut store = match handle_ref.store.lock() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let mut queue = match handle_ref.download_queue.lock() {
            Ok(q) => q,
            Err(_) => return std::ptr::null_mut(),
        };
        let outcome = dispatch_download_report_json_with_queue(&mut store, &mut queue, report_str);
        match outcome {
            DispatchOutcome::Ok { follow_up_json } => {
                drop(queue);
                drop(store);
                handle_ref
                    .rev
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                follow_up_json
            }
            DispatchOutcome::DecodeFailed { .. } => None,
        }
    }; // store lock released

    match follow_up_json {
        Some(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}
