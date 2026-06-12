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
//!   `nmp_free_string`. Never returns NULL for a valid `handle` +
//!   `episode_id` (D6) — an unknown episode yields `[]`.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, lock poison, and serialize failures return
//! `NULL`. Nothing panics across the FFI.

use std::ffi::{c_char, CStr, CString};

use serde::Deserialize;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::store::events::{EventDetail, EventSeverity};

/// Wire shape for [`nmp_app_podcast_record_episode_event`]. Mirrors the Swift
/// `AppStateStore.kernelRecordEpisodeEvent` payload: the host capability (which
/// holds the provider / model / file knowledge the kernel lacks) authors a
/// fully-formed diagnostic line and the kernel just appends it to the episode's
/// log. `severity` is one of `info` | `success` | `warning` | `failure`
/// (unknown ⇒ `info`); `details` is an ordered list of label/value rows.
#[derive(Debug, Deserialize)]
struct RecordEventRequest {
    episode_id: String,
    kind: String,
    #[serde(default)]
    severity: String,
    summary: String,
    #[serde(default)]
    details: Vec<RecordEventDetail>,
}

#[derive(Debug, Deserialize)]
struct RecordEventDetail {
    label: String,
    value: String,
}

/// Record one host-authored pipeline event onto an episode's Diagnostics log.
///
/// The kernel can only see the stages it runs itself (download, chapter/ad
/// identification, the projected transcript). Stages that run in the iOS
/// capability layer — STT with a specific provider, RAG indexing, clip export,
/// playback the host drives — know details the kernel never will. This is the
/// single generic channel for the host to author a fully-formed event so
/// "ANYTHING related to an episode" can land in the one log the user reads.
///
/// `event_json` is a single `RecordEventRequest` object (NOT an array). The
/// call is fire-and-forget: it always returns `NULL` (there is nothing for the
/// caller to read or free) and never bumps `rev` — events ride their own
/// off-snapshot per-episode files, exactly like the kernel-emitted ones. D6:
/// null pointers, bad UTF-8, malformed JSON, and lock poison all degrade to a
/// silent no-op.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_record_episode_event(
    handle: *mut PodcastHandle,
    event_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || event_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_record_episode_event",
        std::ptr::null_mut,
        || {
            let raw = match unsafe { CStr::from_ptr(event_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };

            let req: RecordEventRequest = match serde_json::from_str(raw) {
                Ok(r) => r,
                Err(_) => return std::ptr::null_mut(),
            };

            let handle_ref = unsafe { &*handle };
            if let Ok(mut store) = handle_ref.state.library.store.lock() {
                let details = req
                    .details
                    .into_iter()
                    .map(|d| EventDetail::new(d.label, d.value))
                    .collect();
                store.emit_event(
                    &req.episode_id,
                    &req.kind,
                    EventSeverity::from_wire(&req.severity),
                    req.summary,
                    details,
                );
            }

            std::ptr::null_mut()
        },
    )
}

/// Fetch the JSON-encoded pipeline event log for one episode.
///
/// Returns a malloc-compatible string the caller MUST free via
/// `nmp_free_string`, or `NULL` on any error (D6 degrade-silently).
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_episode_events(
    handle: *mut PodcastHandle,
    episode_id: *const c_char,
) -> *mut c_char {
    if handle.is_null() || episode_id.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_episode_events", std::ptr::null_mut, || {
        let episode_id = match unsafe { CStr::from_ptr(episode_id) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let handle_ref = unsafe { &*handle };
        let events = match handle_ref.state.library.store.lock() {
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
    })
}

#[cfg(test)]
mod tests {
    use super::{RecordEventRequest, RecordEventDetail};
    use crate::store::events::{EpisodeEvent, EventDetail, EventSeverity};

    #[test]
    fn record_event_request_parses_swift_payload() {
        // Exactly what `AppStateStore.kernelRecordEpisodeEvent` posts.
        let json = r#"{
            "episode_id":"7B2A-uuid",
            "kind":"transcript.attempt",
            "severity":"info",
            "summary":"Transcribing audio · ElevenLabs Scribe",
            "details":[{"label":"Service","value":"ElevenLabs Scribe"},
                       {"label":"Audio","value":"local file"}]
        }"#;
        let req: RecordEventRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.episode_id, "7B2A-uuid");
        assert_eq!(req.kind, "transcript.attempt");
        assert_eq!(req.summary, "Transcribing audio · ElevenLabs Scribe");
        assert_eq!(req.details.len(), 2);
        assert_eq!(req.details[0].label, "Service");
        assert_eq!(req.details[0].value, "ElevenLabs Scribe");
    }

    #[test]
    fn record_event_request_tolerates_absent_optionals() {
        // severity + details omitted ⇒ default empty (severity maps to info).
        let json = r#"{"episode_id":"e","kind":"clip.created","summary":"Clip created"}"#;
        let req: RecordEventRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.severity, "");
        assert!(req.details.is_empty());
        assert_eq!(EventSeverity::from_wire(&req.severity), EventSeverity::Info);
    }

    #[test]
    fn severity_from_wire_maps_known_and_unknown() {
        assert_eq!(EventSeverity::from_wire("success"), EventSeverity::Success);
        assert_eq!(EventSeverity::from_wire("warning"), EventSeverity::Warning);
        assert_eq!(EventSeverity::from_wire("failure"), EventSeverity::Failure);
        assert_eq!(EventSeverity::from_wire("info"), EventSeverity::Info);
        assert_eq!(EventSeverity::from_wire("bogus"), EventSeverity::Info);
    }

    // Touch the detail constructor so an unused-import lint never fires if the
    // parse tests above are refactored to stop reading `RecordEventDetail`.
    #[test]
    fn record_event_detail_fields_are_public_within_module() {
        let d = RecordEventDetail {
            label: "k".into(),
            value: "v".into(),
        };
        assert_eq!(d.label, "k");
        assert_eq!(d.value, "v");
    }

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
