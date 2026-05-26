//! Tests for [`super::transcript`] — accept-header mapping, entry projection,
//! parse dispatch, and fetch-handler integration.
//!
//! Extracted from `transcript.rs` to keep that file under the 500-line hard limit.

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
fn project_entries_preserves_speaker_and_timing() {
    let transcript = Transcript::ready(
        "ep-1".to_owned(),
        vec![
            podcast_transcripts::TranscriptEntry {
                start_secs: 0.0,
                end_secs: 1.5,
                speaker: Some("Alice".to_owned()),
                text: "Hello".to_owned(),
                words: None,
            },
            podcast_transcripts::TranscriptEntry {
                start_secs: 1.5,
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
    let projected = project_entries(&transcript);
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].speaker, Some("Alice".to_owned()));
    assert_eq!(projected[0].start_secs, 0.0);
    assert_eq!(projected[0].end_secs, Some(1.5));
    assert_eq!(projected[0].text, "Hello");
    assert_eq!(projected[1].speaker, None);
    assert_eq!(projected[1].end_secs, Some(3.0));
}

#[test]
fn project_entries_drops_zero_end_secs() {
    // The `Text` kind wrapping path emits `end_secs: 0.0` for an untimed
    // single-entry payload; the projection should map that to `None` so
    // the viewer's "no end" fallback kicks in.
    let transcript = Transcript::ready(
        "ep-1".to_owned(),
        vec![podcast_transcripts::TranscriptEntry {
            start_secs: 0.0,
            end_secs: 0.0,
            speaker: None,
            text: "Plain body.".to_owned(),
            words: None,
        }],
        "data:text/plain,".to_owned(),
        TranscriptKind::Text,
        podcast_core::TranscriptSource::Publisher,
    );
    let projected = project_entries(&transcript);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].end_secs, None);
    assert_eq!(projected[0].text, "Plain body.");
}

#[test]
fn text_kind_wraps_body_into_single_entry() {
    let body = "Plain transcript body.";
    let transcript = parse_transcript_body(body, &TranscriptKind::Text, "ep-1", "data:text/plain,")
        .expect("text parse");
    assert_eq!(transcript.entries.len(), 1);
    assert_eq!(transcript.entries[0].text, body);
    let projected = project_entries(&transcript);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].text, body);
}

#[test]
fn html_kind_is_rejected_with_clear_message() {
    let err = parse_transcript_body("<p>hi</p>", &TranscriptKind::Html, "ep-1", "https://ex.com/t.html")
        .expect_err("html should fail");
    assert!(err.contains("html"));
}

#[test]
fn vtt_round_trip_via_parse_and_project() {
    let body = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello\n\n00:00:01.000 --> 00:00:02.000\nworld.\n";
    let transcript =
        parse_transcript_body(body, &TranscriptKind::Vtt, "ep-1", "https://ex.com/t.vtt")
            .expect("vtt parse");
    let projected = project_entries(&transcript);
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].text, "Hello");
    assert_eq!(projected[1].text, "world.");
    assert_eq!(projected[0].end_secs, Some(1.0));
}

#[test]
fn handle_fetch_transcript_stores_entries_and_bumps_rev() {
    use podcast_core::{Episode, Podcast, TranscriptKind};

    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let transcripts = Arc::new(Mutex::new(HashMap::new()));
    let rev = AtomicU64::new(0);

    let podcast = Podcast::new("Show");
    let mut episode = Episode::new(
        podcast.id,
        "https://example.com/feed.xml",
        "guid",
        "Episode",
        url::Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    );
    episode.publisher_transcript_url =
        Some(url::Url::parse("https://example.com/t.vtt").unwrap());
    episode.publisher_transcript_type = Some(TranscriptKind::Vtt);
    let id = episode.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![episode]);

    let body = "WEBVTT\n\n00:00:00.000 --> 00:00:01.500\nHello\n";
    let result = handle_fetch_transcript(
        &store,
        &transcripts,
        &rev,
        id.clone(),
        |_req| Ok(HttpResult::Ok {
            status_code: 200,
            headers: vec![],
            body: body.to_owned(),
        }),
    );

    assert_eq!(result["ok"], true);
    assert_eq!(result["status"], "fetched");
    assert_eq!(rev.load(Ordering::Relaxed), 1);
    let cache = transcripts.lock().unwrap();
    let entries = cache.get(&id).expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].text, "Hello");
    assert_eq!(entries[0].end_secs, Some(1.5));
}

#[test]
fn handle_fetch_transcript_returns_not_available_when_no_url() {
    use podcast_core::{Episode, Podcast};

    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let transcripts = Arc::new(Mutex::new(HashMap::new()));
    let rev = AtomicU64::new(0);

    let podcast = Podcast::new("Show");
    let episode = Episode::new(
        podcast.id,
        "https://example.com/feed.xml",
        "guid",
        "Episode",
        url::Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    );
    let id = episode.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![episode]);

    let result = handle_fetch_transcript(
        &store,
        &transcripts,
        &rev,
        id,
        |_req| panic!("fetch must not run when no URL is available"),
    );

    assert_eq!(result["ok"], true);
    assert_eq!(result["status"], "not_available");
    assert_eq!(rev.load(Ordering::Relaxed), 0);
    assert!(transcripts.lock().unwrap().is_empty());
}
