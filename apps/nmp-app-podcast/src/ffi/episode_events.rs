//! `nmp_app_podcast_episode_events` — lazy per-episode pipeline event log read.
//!
//! iOS calls this when the Episode Diagnostics sheet opens (and on the `rev`
//! changes that sheet already observes) to fetch the kernel's pipeline event
//! log for one episode. The events deliberately do **not** ride the library
//! snapshot: that snapshot is fully JSON-decoded on the main thread on every
//! `rev` bump (3.9 MB / 35 ms at ~3.6k episodes), so folding a per-episode
//! event array into it would regress the hot path for a sheet that is open a
//! fraction of the time. A single-episode getter keeps the cost paid only when
//! the sheet is on screen.
//!
//! ## Wire protocol
//!
//! * **`episode_id`**: a nul-terminated hyphenated UUID string.
//! * **Return value**: a heap-allocated nul-terminated JSON **array** of
//!   `EpisodeEvent` objects (possibly empty `[]`), decoded on the Swift side
//!   straight into `[EpisodeAuditEvent]`. The caller MUST free the pointer via
//!   `nmp_app_free_string`. Never returns NULL for a valid `handle` +
//!   `episode_id` (D6) — an unknown episode yields `[]`.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, lock poison, and serialize failures return
//! `NULL`. Nothing panics across the FFI.

use std::ffi::{c_char, CStr, CString};

use super::handle::PodcastHandle;

/// Fetch the JSON-encoded pipeline event log for one episode.
///
/// Returns a malloc-compatible string the caller MUST free via
/// `nmp_app_free_string`, or `NULL` on any error (D6 degrade-silently).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_episode_events(
    handle: *mut PodcastHandle,
    episode_id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || episode_id.is_null() {
        return std::ptr::null_mut();
    }

    let episode_id = match unsafe { CStr::from_ptr(episode_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let handle_ref = unsafe { &*handle };
    let events = match handle_ref.store.lock() {
        Ok(mut store) => store.episode_events(episode_id),
        Err(_) => return std::ptr::null_mut(),
    };

    match serde_json::to_string(&events) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use crate::store::events::{EpisodeEvent, EventDetail};

    #[test]
    fn events_serialize_to_swift_audit_shape() {
        // The Swift `EpisodeAuditEvent` decoder reads `episodeID` (camelCase),
        // `timestamp`, `kind`, `severity`, `summary`, and `details[{label,value}]`.
        let event = EpisodeEvent {
            id: "evt-1".to_owned(),
            episode_id: "ep-1".to_owned(),
            timestamp: "2026-06-07T07:36:35Z".to_owned(),
            kind: "download.requested".to_owned(),
            severity: "info".to_owned(),
            summary: "queued".to_owned(),
            details: vec![EventDetail::new("URL", "https://x/y.mp3")],
        };
        let json = serde_json::to_string(&[event]).unwrap();
        assert!(json.contains("\"episodeID\":\"ep-1\""));
        assert!(json.contains("\"kind\":\"download.requested\""));
        assert!(json.contains("\"label\":\"URL\""));
        assert!(json.starts_with('['));
    }
}
