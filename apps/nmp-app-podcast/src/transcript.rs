//! Transcript fetch helpers used by `PodcastHostOpHandler::handle_fetch_transcript`.
//!
//! Three small pure-data helpers, factored out of `host_op_handler.rs` so the
//! handler file stays inside the 500-line hard limit (AGENTS.md). The crate
//! that owns each piece of behaviour is kept narrow:
//!
//! * Parsing itself lives in `podcast-transcripts` (no I/O, pure bytes-in /
//!   `Transcript`-out).
//! * The host op handler decides *when* to call the parsers; this module
//!   decides *which* parser (and *what* `Accept` header) goes with which
//!   [`TranscriptKind`], plus how to collapse the parsed `Transcript` into
//!   the single plain-text blob the iOS sheet renders.

use std::sync::{Arc, Mutex};

use podcast_core::TranscriptKind;
use podcast_feeds::http::{HttpRequest, HttpResult};
use podcast_transcripts::{
    parse_podcasting_json, parse_srt, parse_vtt, Transcript, TranscriptEntry,
};

use crate::store::PodcastStore;

pub(crate) enum FetchTranscriptOutcome {
    Stored,
    NotAvailable,
}

pub(crate) fn fetch_and_store_transcript(
    store: &Arc<Mutex<PodcastStore>>,
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
    let plain_text = join_transcript_text(&transcript);

    let mut store = store.lock().map_err(|_| "store poisoned".to_owned())?;
    store.set_transcript(episode_id, plain_text);
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
        // Return a clear diagnostic rather than falling back to a wrong
        // parser (which would silently produce empty text).
        TranscriptKind::Html => Err("html transcripts not yet supported".to_owned()),
        // Plain-text transcripts: there's no parser to invoke — every line
        // is rendered as-is. Wrap the body in a single-entry Transcript so
        // the join step downstream works uniformly across kinds.
        TranscriptKind::Text => Ok(Transcript::ready(
            episode_id.to_owned(),
            vec![TranscriptEntry {
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

/// Join a parsed transcript's entries into a single plain-text blob.
///
/// One entry per line, no speaker labels or timestamps — the iOS sheet
/// renders this with `Text(text)` and selectable text, so the human-readable
/// flow takes precedence over machine-parseable structure.
pub(crate) fn join_transcript_text(transcript: &Transcript) -> String {
    transcript
        .entries
        .iter()
        .map(|e| e.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
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
mod tests {
    use super::*;

    #[test]
    fn accept_header_per_kind() {
        assert_eq!(accept_header(&TranscriptKind::Vtt), "text/vtt, text/plain, */*");
        assert_eq!(accept_header(&TranscriptKind::Srt), "application/x-subrip, text/plain, */*");
        assert_eq!(accept_header(&TranscriptKind::Json), "application/json, */*");
        assert_eq!(accept_header(&TranscriptKind::Html), "text/html, */*");
        assert_eq!(accept_header(&TranscriptKind::Text), "text/plain, */*");
    }

    #[test]
    fn join_text_skips_empty_entries() {
        let transcript = Transcript::ready(
            "ep-1".to_owned(),
            vec![
                TranscriptEntry {
                    start_secs: 0.0,
                    end_secs: 1.0,
                    speaker: None,
                    text: "Hello".to_owned(),
                    words: None,
                },
                TranscriptEntry {
                    start_secs: 1.0,
                    end_secs: 2.0,
                    speaker: None,
                    text: "   ".to_owned(),
                    words: None,
                },
                TranscriptEntry {
                    start_secs: 2.0,
                    end_secs: 3.0,
                    speaker: None,
                    text: "world.".to_owned(),
                    words: None,
                },
            ],
            "https://ex.com/t.vtt".to_owned(),
            TranscriptKind::Vtt,
            podcast_core::TranscriptSource::Publisher,
        );
        assert_eq!(join_transcript_text(&transcript), "Hello\nworld.");
    }

    #[test]
    fn text_kind_wraps_body_into_single_entry() {
        let body = "Plain transcript body.";
        let transcript = parse_transcript_body(body, &TranscriptKind::Text, "ep-1", "data:text/plain,")
            .expect("text parse");
        assert_eq!(transcript.entries.len(), 1);
        assert_eq!(transcript.entries[0].text, body);
        assert_eq!(join_transcript_text(&transcript), body);
    }

    #[test]
    fn html_kind_is_rejected_with_clear_message() {
        let err = parse_transcript_body("<p>hi</p>", &TranscriptKind::Html, "ep-1", "https://ex.com/t.html")
            .expect_err("html should fail");
        assert!(err.contains("html"));
    }

    #[test]
    fn vtt_round_trip_via_parse_and_join() {
        let body = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello\n\n00:00:01.000 --> 00:00:02.000\nworld.\n";
        let transcript =
            parse_transcript_body(body, &TranscriptKind::Vtt, "ep-1", "https://ex.com/t.vtt")
                .expect("vtt parse");
        assert_eq!(join_transcript_text(&transcript), "Hello\nworld.");
    }
}
