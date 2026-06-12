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
//!   `nmp_free_string`. This is how the Rust queue starts the next
//!   waiting item after iOS reports `Completed`, `Failed`, or `Cancelled`.
//!
//! ## Lock discipline
//!
//! Acquires the `PodcastStore` and `DownloadQueue` locks, dispatches the
//! projection, snapshots the (small) download queue while the lock is held,
//! drops both locks, then bumps the global `rev` **only when the report
//! changed durable library state**. Matches the audio shim's discipline —
//! never hold locks across the rev bump, never panic across the FFI.
//!
//! ## Rev discipline — why progress no longer bumps `rev`
//!
//! Download *progress* ticks fire ~1 Hz per active download. Bumping the
//! global `rev` on each one forced the Swift side to pull and JSON-decode the
//! ENTIRE library snapshot (plus O(N×M) content/spotlight hashes) on the main
//! thread — empirically the worst CPU/heat path during downloads. Progress
//! changes only transient queue state, so it must NOT bump `rev`. Instead the
//! response carries the fresh [`DownloadQueueSnapshot`] inline; Swift updates
//! its always-fresh `downloadSnapshot` from it without touching the library.
//! Only a completion/cancellation (which flips `Episode.downloadState` and is
//! reported by `durable_changed`) bumps `rev` so the library projection runs.
//!
//! ## Wire protocol (response)
//!
//! On success the FFI returns nul-terminated JSON of:
//! ```json
//! { "follow_up": <DownloadCommand or omitted>,
//!   "downloads": <DownloadQueueSnapshot or omitted>,
//!   "durable_changed": <bool> }
//! ```
//! The caller MUST free the pointer via `nmp_free_string`.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, lock poison, decode failures, and serialize
//! failures all return `NULL` (treated by iOS as "nothing actionable, don't
//! pull"). Nothing panics across the FFI.

use std::ffi::{c_char, CStr, CString};

use serde::Serialize;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use super::projections::DownloadQueueSnapshot;
use super::snapshot_downloads::build_downloads_snapshot;
use crate::capability::dispatch::dispatch_download_report_json_with_queue;

/// JSON response shape returned to the Swift download-report channel. Fields
/// are decoded on the Swift side with `convertFromSnakeCase`.
#[derive(Serialize)]
struct DownloadReportResponse {
    /// JSON of the follow-up `DownloadCommand` when the report freed a queue
    /// slot; omitted otherwise. Carried as a *string* (not a nested object) so
    /// Swift decodes it with a plain decoder — `DownloadCommand` uses explicit
    /// `episode_id` coding keys that a `convertFromSnakeCase` pass would break.
    #[serde(skip_serializing_if = "Option::is_none")]
    follow_up: Option<String>,
    /// Fresh download-queue snapshot so Swift can update its live
    /// `downloadSnapshot` without pulling the full library. Omitted when the
    /// queue has no active/queued/paused/failed rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    downloads: Option<DownloadQueueSnapshot>,
    /// `true` when the report flipped durable library state (a completed or
    /// cancelled download). Swift pulls the full snapshot only when this is set.
    durable_changed: bool,
}

/// Deliver a JSON-encoded `DownloadReport` to the Rust `PodcastStore` and
/// return the JSON-encoded [`DownloadReportResponse`].
///
/// Returns a malloc-compatible string the caller MUST free via
/// `nmp_free_string`, or `NULL` on any error (D6 degrade-silently).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_download_report(
    handle: *mut PodcastHandle,
    report_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || report_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_download_report",
        std::ptr::null_mut,
        || {
            let report_str = match unsafe { CStr::from_ptr(report_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };

            let handle_ref = unsafe { &*handle };
            let response = {
                // Step 14: download_queue sourced from state.playback.downloads
                // via the same Arc<Mutex<DownloadQueue>> — lock topology unchanged.
                let mut store = match handle_ref.state.library.store.lock() {
                    Ok(s) => s,
                    Err(_) => return std::ptr::null_mut(),
                };
                let mut queue = match handle_ref.state.playback.downloads.lock() {
                    Ok(q) => q,
                    Err(_) => return std::ptr::null_mut(),
                };
                let dispatch =
                    dispatch_download_report_json_with_queue(&mut store, &mut queue, report_str);
                if dispatch.decode_failed {
                    return std::ptr::null_mut();
                }
                // Snapshot the queue while the lock is held so Swift gets live
                // progress without a second FFI round-trip.
                let downloads = build_downloads_snapshot(&queue);
                drop(queue);
                drop(store);
                // Only durable library changes (completion/cancellation) bump the
                // global `rev`; progress ticks ride the inline `downloads` field.
                // A completed/cancelled download changes the episode's
                // download_path/file_size_bytes in the `podcast.library` payload,
                // so route the delta there.
                handle_ref
                    .bump_snapshot_rev_domain_if(crate::state::Domain::Library, dispatch.durable_changed);
                DownloadReportResponse {
                    follow_up: dispatch.follow_up_json,
                    downloads,
                    durable_changed: dispatch.durable_changed,
                }
            }; // locks released

            match serde_json::to_string(&response) {
                Ok(json) => match CString::new(json) {
                    Ok(c) => c.into_raw(),
                    Err(_) => std::ptr::null_mut(),
                },
                Err(_) => std::ptr::null_mut(),
            }
        },
    )
}
