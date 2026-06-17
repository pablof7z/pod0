//! `nmp_app_podcast_transcript_report` — iOS→Rust transcript-ready report.
//!
//! iOS fires this entry point after `TranscriptIngestService` successfully
//! completes an STT or publisher-transcript pass. Rust stores the plain-text
//! transcript so AI features (wiki, chapters, RAG context, agent chat) can
//! access it without going through Swift's TranscriptStore.
//!
//! ## Wire shapes
//!
//! ### New (slice 5a) — timed form:
//!
//! ```json
//! {
//!   "episode_id": "<uuid-string>",
//!   "entries":    [{"start_secs":0.0,"end_secs":5.2,"text":"Hello","speaker":"spk_0"}, …],
//!   "source":     "ElevenLabs Scribe"
//! }
//! ```
//!
//! When `entries` is present the kernel stores both the timed entries (for
//! time-aware RAG chunking) and the plain text derived from them (for AI
//! features that consume `transcript_for`).
//!
//! ### Legacy — plain-text form (back-compat, always accepted):
//!
//! ```json
//! {
//!   "episode_id": "<uuid-string>",
//!   "text":       "<full plain-text transcript>",
//!   "source":     "ElevenLabs Scribe"
//! }
//! ```
//!
//! Legacy callers continue to work unchanged; `index_episode` falls back to
//! `chunk_transcript_text` (start_secs=0.0) when no timed entries are stored.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, missing fields, and lock poison all return
//! `NULL`. Nothing panics across the FFI.

use std::ffi::{c_char, CStr};

use serde::Deserialize;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

/// One timed segment in the structured `transcript_report` payload.
///
/// Maps directly to `podcast_transcripts::TranscriptEntry`; deserialised
/// from the iOS `Transcript.Segment` JSON.
#[derive(Deserialize)]
struct TimedEntryPayload {
    start_secs: f64,
    end_secs: f64,
    text: String,
    #[serde(default)]
    speaker: Option<String>,
}

impl From<TimedEntryPayload> for podcast_transcripts::TranscriptEntry {
    fn from(p: TimedEntryPayload) -> Self {
        Self {
            start_secs: p.start_secs,
            end_secs: p.end_secs,
            speaker: p.speaker,
            text: p.text,
            words: None,
        }
    }
}

#[derive(Deserialize)]
struct TranscriptReport {
    episode_id: String,
    /// Legacy plain-text form.  Present when only text was supplied.
    /// Back-compat — all existing callers continue to work.
    #[serde(default)]
    text: Option<String>,
    /// New timed form (slice 5a).  When present the kernel stores both timed
    /// entries (enabling time-aware RAG chunking) and plain text derived from
    /// the entries (for wiki / chapters / agent-chat features).
    #[serde(default)]
    entries: Option<Vec<TimedEntryPayload>>,
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
    ffi_guard(
        "nmp_app_podcast_transcript_report",
        std::ptr::null_mut,
        || {
            let report_str = match unsafe { CStr::from_ptr(report_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };

            let report: TranscriptReport = match serde_json::from_str(report_str) {
                Ok(r) => r,
                Err(_) => return std::ptr::null_mut(),
            };

            let handle_ref = unsafe { &*handle };
            let episode_id = report.episode_id.clone();
            if let Ok(mut s) = handle_ref.state.library.store.lock() {
                let source = report.source.clone();

                // Determine plain text and (optionally) timed entries.
                //
                // Preference order:
                //  1. Timed form ("entries"): convert entries, store them, derive text.
                //  2. Legacy form ("text"): store text only; no timing data.
                        let (plain_text, maybe_entries) = match (report.entries, report.text) {
                    (Some(entries), _) => {
                        // New timed form — derive plain text from entries.
                        let text = entries
                            .iter()
                            .map(|e| e.text.as_str())
                            .collect::<Vec<_>>()
                            .join(" ");
                        let typed: Vec<podcast_transcripts::TranscriptEntry> =
                            entries.into_iter().map(Into::into).collect();
                        (text, Some(typed))
                    }
                    (None, Some(text)) => (text, None),
                    (None, None) => return std::ptr::null_mut(),
                };

                let char_count = plain_text.chars().count();

                // Store timed entries when present (enables time-aware RAG chunking).
                if let Some(entries) = maybe_entries {
                    s.set_timed_transcript(report.episode_id.clone(), entries);
                }
                // Always store plain text so wiki / chapters / agent-chat features
                // that call `transcript_for` continue to work.
                s.set_transcript(report.episode_id.clone(), plain_text);

                // Stage 3 → 4 of the pipeline: the transcript landed. Record it so
                // the Diagnostics sheet shows the transcript stage completing and the
                // event log reflects the moment chapter/ad identification can begin.
                // Name the service when the caller supplied it so the log reads
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
            let refined_clips = handle_ref.state.clips.refine_pending_for_episode(&episode_id);
            for clip in refined_clips {
                crate::social_publish_handler::publish_clip_highlight_if_user_visible(
                    handle_ref.app,
                    &handle_ref.state.library.identity,
                    &handle_ref.state.library.store,
                    &clip,
                    "transcript_report",
                );
            }
            // Bump rev so the next snapshot tick surfaces the new transcript_entries
            // and transcript fields on EpisodeSummary.
            handle_ref.bump_snapshot_rev();

            std::ptr::null_mut()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{TimedEntryPayload, TranscriptReport};

    #[test]
    fn legacy_plain_text_report_round_trips() {
        let json = r#"{"episode_id":"abc-123","text":"Hello world"}"#;
        let r: TranscriptReport = serde_json::from_str(json).unwrap();
        assert_eq!(r.episode_id, "abc-123");
        assert_eq!(r.text.as_deref(), Some("Hello world"));
        assert!(r.entries.is_none());
    }

    #[test]
    fn timed_entries_report_deserializes() {
        let json = r#"{
            "episode_id": "abc-456",
            "entries": [
                {"start_secs": 0.0, "end_secs": 5.2, "text": "Hello world", "speaker": "spk_0"},
                {"start_secs": 5.2, "end_secs": 10.1, "text": "Second sentence"}
            ],
            "source": "ElevenLabs Scribe"
        }"#;
        let r: TranscriptReport = serde_json::from_str(json).unwrap();
        assert_eq!(r.episode_id, "abc-456");
        assert!(r.text.is_none(), "new form carries no top-level text");
        let entries = r.entries.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].start_secs, 0.0);
        assert_eq!(entries[0].end_secs, 5.2);
        assert_eq!(entries[0].text, "Hello world");
        assert_eq!(entries[0].speaker.as_deref(), Some("spk_0"));
        assert_eq!(entries[1].start_secs, 5.2);
        assert!(entries[1].speaker.is_none());
        assert_eq!(r.source.as_deref(), Some("ElevenLabs Scribe"));
    }

    #[test]
    fn timed_entry_converts_to_transcript_entry() {
        let p = TimedEntryPayload {
            start_secs: 1.5,
            end_secs: 3.7,
            text: "test text".to_owned(),
            speaker: Some("spk_0".to_owned()),
        };
        let e: podcast_transcripts::TranscriptEntry = p.into();
        assert_eq!(e.start_secs, 1.5);
        assert_eq!(e.end_secs, 3.7);
        assert_eq!(e.text, "test text");
        assert_eq!(e.speaker.as_deref(), Some("spk_0"));
        assert!(e.words.is_none());
    }
}
