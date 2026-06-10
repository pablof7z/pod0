//! `nmp_app_podcast_transcript_report` — iOS→Rust transcript-ready report.
//!
//! iOS fires this entry point after `TranscriptIngestService` successfully
//! completes an STT or publisher-transcript pass. Rust stores the plain-text
//! transcript so AI features (wiki, chapters, RAG context, agent chat) can
//! access it without going through Swift's TranscriptStore.
//!
//! Wire shape:
//!
//! ```json
//! {
//!   "episode_id": "<uuid-string>",
//!   "text":       "<full plain-text transcript>",
//!   "source":     "ElevenLabs Scribe"   // optional — names the service in the log
//! }
//! ```
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, missing fields, and lock poison all return
//! `NULL`. Nothing panics across the FFI.

use std::ffi::{c_char, CStr};

use serde::Deserialize;

use super::handle::PodcastHandle;

#[derive(Deserialize)]
struct TranscriptReport {
    episode_id: String,
    text: String,
    /// Human-readable name of the service that produced the transcript
    /// (e.g. "ElevenLabs Scribe", "Apple Native (on-device)", "Publisher
    /// feed"). Optional for back-compat with older callers; when present it is
    /// surfaced as the `Service` detail on the `transcript.ready` event so the
    /// Diagnostics log says *which* service did the work, not just that it
    /// finished.
    #[serde(default)]
    source: Option<String>,
}

/// Deliver a JSON-encoded transcript report to the kernel.
/// The transcript text is stored in the Rust `PodcastStore` so AI features
/// can access it without going through Swift's TranscriptStore.
/// Always returns NULL.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_transcript_report(
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

    let report: TranscriptReport = match serde_json::from_str(report_str) {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    let handle_ref = unsafe { &*handle };
    if let Ok(mut s) = handle_ref.store.lock() {
        let char_count = report.text.chars().count();
        let source = report.source.clone();
        s.set_transcript(report.episode_id.clone(), report.text);
        // Stage 3 → 4 of the pipeline: the transcript landed. Record it so the
        // Diagnostics sheet shows the transcript stage completing and the event
        // log reflects the moment chapter/ad identification can begin. Name the
        // service when the caller supplied it so the log reads
        // "Transcript ready · ElevenLabs Scribe" rather than a bare count.
        let mut details = Vec::with_capacity(2);
        if let Some(service) = source.as_deref() {
            details.push(crate::store::events::EventDetail::new("Service", service));
        }
        details.push(crate::store::events::EventDetail::new(
            "Characters",
            char_count.to_string(),
        ));
        let summary = match source.as_deref() {
            Some(service) => format!("Transcript ready · {service}"),
            None => "Transcript ready".to_owned(),
        };
        s.emit_event(
            &report.episode_id,
            crate::store::events::stage::TRANSCRIPT_READY,
            crate::store::events::EventSeverity::Success,
            summary,
            details,
        );
    }
    // Bump rev so the next snapshot tick surfaces the new transcript_entries
    // and transcript fields on EpisodeSummary.
    handle_ref.bump_snapshot_rev();

    std::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::TranscriptReport;

    #[test]
    fn transcript_report_round_trips() {
        let json = r#"{"episode_id":"abc-123","text":"Hello world"}"#;
        let r: TranscriptReport = serde_json::from_str(json).unwrap();
        assert_eq!(r.episode_id, "abc-123");
        assert_eq!(r.text, "Hello world");
    }
}
