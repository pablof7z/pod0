//! Transcript fetch helpers used by `PodcastHostOpHandler::handle_fetch_transcript`.
//!
//! Lives outside `host_op_handler.rs` so the handler file stays inside the
//! 500-line hard limit (AGENTS.md). The crate that owns each piece of
//! behaviour is kept narrow:
//!
//! * Parsing itself lives in `podcast-transcripts` (no I/O, pure bytes-in /
//!   `Transcript`-out).
//! * The host op handler decides *when* to call the parsers; this module
//!   decides *which* parser (and *what* `Accept` header) goes with which
//!   [`TranscriptKind`], plus how to project the parsed `Transcript` into
//!   the [`TranscriptEntry`] rows the iOS viewer renders.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use podcast_core::TranscriptKind;
use podcast_feeds::http::{HttpRequest, HttpResult};
use podcast_transcripts::{
    parse_podcasting_json, parse_srt, parse_vtt, Transcript,
};

use crate::ffi::projections::TranscriptEntry;
use crate::store::PodcastStore;

enum FetchTranscriptOutcome {
    Stored,
    NotAvailable,
}

/// Resolve the publisher transcript URL for `episode_id`, dispatch HTTP via
/// `fetch`, parse the bytes through the [`TranscriptKind`]-matched parser,
/// then store the projected [`TranscriptEntry`] rows in the per-handle cache
/// so the next snapshot tick surfaces them.
///
/// Returns a JSON envelope mirroring the rest of the host-op handlers:
/// `{"ok":true,"status":"fetched"}` on success, `{"ok":true,"status":"not_available"}`
/// when the episode lacks a publisher URL, or `{"ok":false,"error":"…"}` on
/// HTTP / parse failure.
pub(crate) fn handle_fetch_transcript(
    store: &Arc<Mutex<PodcastStore>>,
    transcripts: &Arc<Mutex<HashMap<String, Vec<TranscriptEntry>>>>,
    rev: &AtomicU64,
    episode_id: String,
    fetch: impl FnOnce(&HttpRequest) -> Result<HttpResult, String>,
) -> serde_json::Value {
    match fetch_and_store_transcript(store, transcripts, episode_id, fetch) {
        Ok(FetchTranscriptOutcome::Stored) => {
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true, "status": "fetched"})
        }
        Ok(FetchTranscriptOutcome::NotAvailable) => {
            serde_json::json!({"ok": true, "status": "not_available"})
        }
        Err(e) => serde_json::json!({"ok": false, "error": e}),
    }
}

fn fetch_and_store_transcript(
    store: &Arc<Mutex<PodcastStore>>,
    transcripts: &Arc<Mutex<HashMap<String, Vec<TranscriptEntry>>>>,
    episode_id: String,
    fetch: impl FnOnce(&HttpRequest) -> Result<HttpResult, String>,
) -> Result<FetchTranscriptOutcome, String> {
    let (url, kind) = {
        let store = store.lock().map_err(|_| "store poisoned".to_owned())?;
        match store.episode_publisher_transcript(&episode_id) {
            Some(info) => info,
            None => return Ok(FetchTranscriptOutcome::NotAvailable),
        }
    };

    let req = HttpRequest::get(url.clone(), [("Accept", accept_header(&kind))]);
    let http_result = fetch(&req)?;
    let body = match http_result {
        HttpResult::Ok { body, .. } => body,
        HttpResult::Error { message } => return Err(message),
    };
    let transcript = parse_transcript_body(&body, &kind, &episode_id, &url)?;
    let entries = project_entries(&transcript);

    let mut cache = transcripts.lock().map_err(|_| "transcripts poisoned".to_owned())?;
    cache.insert(episode_id, entries);
    Ok(FetchTranscriptOutcome::Stored)
}

/// Parse a transcript response body using the parser matching `kind`.
///
/// Returns the parsed [`Transcript`] on success, or a short diagnostic string
/// on failure (so the caller can surface it through the `ok=false` envelope).
pub(crate) fn parse_transcript_body(
    body: &str,
    kind: &TranscriptKind,
    episode_id: &str,
    source_url: &str,
) -> Result<Transcript, String> {
    match kind {
        TranscriptKind::Vtt => parse_vtt(body, episode_id, source_url)
            .map_err(|e| format!("transcript parse: {e}")),
        TranscriptKind::Srt => parse_srt(body, episode_id, source_url)
            .map_err(|e| format!("transcript parse: {e}")),
        TranscriptKind::Json => parse_podcasting_json(body.as_bytes(), episode_id, source_url)
            .map_err(|e| format!("transcript parse: {e}")),
        // HTML transcripts are not yet supported by the parsing layer.
        TranscriptKind::Html => Err("html transcripts not yet supported".to_owned()),
        // Plain-text transcripts: wrap the body in a single untimed entry so
        // the iOS viewer still has something to render.
        TranscriptKind::Text => Ok(Transcript::ready(
            episode_id.to_owned(),
            vec![podcast_transcripts::TranscriptEntry {
                start_secs: 0.0,
                end_secs: 0.0,
                speaker: None,
                text: body.to_owned(),
                words: None,
            }],
            source_url.to_owned(),
            TranscriptKind::Text,
            podcast_core::TranscriptSource::Publisher,
        )),
    }
}

/// Project parsed `podcast_transcripts::TranscriptEntry` rows into the narrow
/// FFI shape consumed by the iOS shell.
///
/// `end_secs` is mapped to `None` when the source provides the sentinel
/// `0.0` value (the [`TranscriptKind::Text`] wrapping path above uses that)
/// so the viewer's "no end" highlight fallback can kick in. Per-word
/// timestamps are dropped — the M14 viewer renders segment-level only.
pub(crate) fn project_entries(transcript: &Transcript) -> Vec<TranscriptEntry> {
    transcript
        .entries
        .iter()
        .map(|e| TranscriptEntry {
            start_secs: e.start_secs,
            end_secs: if e.end_secs > e.start_secs { Some(e.end_secs) } else { None },
            speaker: e.speaker.clone(),
            text: e.text.clone(),
        })
        .collect()
}

/// Pick an `Accept` header that matches the publisher transcript kind.
///
/// VTT/SRT are plain text; JSON announces the Podcasting 2.0 transcript
/// MIME but defaults to a permissive `*/*` because some publishers serve
/// transcripts behind CDNs that don't honour content negotiation.
pub(crate) fn accept_header(kind: &TranscriptKind) -> &'static str {
    match kind {
        TranscriptKind::Vtt => "text/vtt, text/plain, */*",
        TranscriptKind::Srt => "application/x-subrip, text/plain, */*",
        TranscriptKind::Json => "application/json, */*",
        TranscriptKind::Html => "text/html, */*",
        TranscriptKind::Text => "text/plain, */*",
    }
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
