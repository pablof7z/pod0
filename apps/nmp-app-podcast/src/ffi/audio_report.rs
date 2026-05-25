//! `nmp_app_podcast_audio_report` — async iOS→Rust audio-report channel.
//!
//! The iOS `AudioCapability` fires this FFI entry point whenever it has a new
//! `AudioReport` to deliver (time ticks, track-end, sleep-timer-fired, …).
//! Rust applies the report to the `PlayerActor` state machine and returns any
//! follow-up `AudioCommand` the iOS side should immediately execute.
//!
//! ## Wire protocol
//!
//! * **Request**: `report_json` is a JSON-encoded [`crate::capability::AudioReport`].
//! * **Response**: heap-allocated nul-terminated JSON of an
//!   [`crate::capability::AudioCommand`], or `NULL` when no follow-up is needed.
//!   The caller MUST free the returned pointer via `nmp_app_free_string`.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, lock poison, and decode failures all return
//! `NULL` (treated by iOS as "no follow-up command"). Nothing panics.

use std::ffi::{c_char, CStr, CString};
use std::time::SystemTime;

use super::handle::PodcastHandle;
use crate::capability::dispatch::dispatch_audio_report_json;
use crate::capability::dispatch::DispatchOutcome;

/// Deliver a JSON-encoded `AudioReport` to the Rust `PlayerActor` and return
/// the JSON-encoded follow-up `AudioCommand`, if any.
///
/// Returns a malloc-compatible string the caller MUST free via `nmp_app_free_string`,
/// or `NULL` when no follow-up is needed (or on any error).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_audio_report(
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
        let mut actor = match handle_ref.player_actor.lock() {
            Ok(a) => a,
            Err(_) => return std::ptr::null_mut(),
        };
        let outcome = dispatch_audio_report_json(&mut actor, report_str, SystemTime::now());
        match outcome {
            DispatchOutcome::Ok { follow_up_json } => {
                drop(actor); // release lock before rev bump
                handle_ref.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                follow_up_json
            }
            DispatchOutcome::DecodeFailed { .. } => None,
        }
    }; // player_actor lock released

    match follow_up_json {
        Some(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    // Integration-level tests that exercise the full FFI round-trip live in
    // `snapshot.rs`'s test section alongside the handle setup. Unit tests for
    // the dispatch logic itself live in `capability/dispatch.rs`.
}
