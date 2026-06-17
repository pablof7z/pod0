//! Tests for [`super::transcript_refine`] and `handle_auto_snip` with transcripts.
//!
//! Extracted from `clip_handler_tests.rs` (S3a SLICE) to keep both files under
//! the 500-line hard limit. The parent `mod tests` in `clip_handler_tests.rs`
//! references this via `#[path = "clip_handler_transcript_tests.rs"]`.

use super::*;
use podcast_core::{Chapter, Episode, EpisodeId, Podcast};
use podcast_transcripts::TranscriptEntry;
use url::Url;

// ── helpers ────────────────────────────────────────────────────────────────────

fn entry(start: f64, end: f64, text: &str) -> TranscriptEntry {
    TranscriptEntry {
        start_secs: start,
        end_secs: end,
        speaker: None,
        text: text.to_owned(),
        words: None,
    }
}

fn entries_vec(pairs: &[(f64, f64)]) -> Vec<TranscriptEntry> {
    pairs
        .iter()
        .enumerate()
        .map(|(i, &(s, e))| entry(s, e, &format!("utterance {i}")))
        .collect()
}

fn build_store_with_transcript(
    ep_id: &str,
    duration: Option<f64>,
    chapters: Option<Vec<Chapter>>,
    transcript: Option<Vec<TranscriptEntry>>,
) -> Arc<Mutex<PodcastStore>> {
    let mut podcast = Podcast::new("Transcript Show");
    podcast.feed_url = Some(Url::parse("https://ex.com/rss").unwrap());
    let mut episode = Episode::new(
        podcast.id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        "Transcript Episode",
        Url::parse("https://ex.com/ep.mp3").unwrap(),
        chrono::Utc::now(),
    );
    episode.id = EpisodeId(Uuid::parse_str(ep_id).unwrap());
    episode.duration_secs = duration;
    episode.chapters = chapters;

    let store = Arc::new(Mutex::new(PodcastStore::new()));
    {
        let mut s = store.lock().unwrap();
        s.subscribe(podcast, vec![episode]);
        if let Some(entries) = transcript {
            s.set_timed_transcript(ep_id.to_owned(), entries);
        }
    }
    store
}

// ── transcript_refine pure-function tests ──────────────────────────────────────

#[test]
fn refine_none_entries_returns_none() {
    // No transcript — no refinement, caller keeps pre-refine range.
    assert_eq!(transcript_refine(60.0, 120.0, None, Some(300.0)), None);
}

#[test]
fn refine_empty_entries_returns_none() {
    // Some(vec![]) — no refinement, same as None.
    assert_eq!(
        transcript_refine(60.0, 120.0, Some(&[]), Some(300.0)),
        None
    );
}

#[test]
fn refine_mid_utterance_tap_recovers_full_utterances() {
    // Three entries: [10,30), [30,60), [60,90).
    // start=40 → snaps back to 30 (entry containing 40 starts at 30).
    // end=75 → snaps forward to 90 (entry containing 75 ends at 90).
    let es = entries_vec(&[(10.0, 30.0), (30.0, 60.0), (60.0, 90.0)]);
    let r = transcript_refine(40.0, 75.0, Some(&es), Some(300.0));
    let (s, e) = r.expect("should refine");
    assert!((s - 30.0).abs() < 1e-9, "start snapped back to 30, got {s}");
    assert!((e - 90.0).abs() < 1e-9, "end snapped forward to 90, got {e}");
}

#[test]
fn refine_start_before_first_entry_snaps_to_first() {
    // start=2, first entry at 10 → snap start to 10.
    let es = entries_vec(&[(10.0, 30.0), (30.0, 60.0)]);
    let r = transcript_refine(2.0, 50.0, Some(&es), Some(300.0));
    let (s, _e) = r.expect("should refine");
    assert!((s - 10.0).abs() < 1e-9, "start snapped to first entry start");
}

#[test]
fn refine_end_past_last_entry_snaps_to_last_end() {
    // end=400, last entry ends at 90 → snap end to 90.
    let es = entries_vec(&[(10.0, 30.0), (60.0, 90.0)]);
    let r = transcript_refine(20.0, 400.0, Some(&es), Some(300.0));
    let (_s, e) = r.expect("should refine");
    // end clamped to duration (300) OR last entry end (90) — whichever is tighter.
    // clamp_duration(90, Some(300)) → 90. So e = 90.
    assert!((e - 90.0).abs() < 1e-9, "end snapped to last entry end (90)");
}

#[test]
fn refine_end_past_last_entry_clamped_to_duration() {
    // end=400, last entry ends at 500, duration=300 → snap to 500, clamp to 300.
    let es = entries_vec(&[(10.0, 30.0), (60.0, 500.0)]);
    let r = transcript_refine(20.0, 400.0, Some(&es), Some(300.0));
    let (_s, e) = r.expect("should refine");
    assert!((e - 300.0).abs() < 1e-9, "end clamped to duration 300");
}

#[test]
fn refine_unsorted_entries_sorted_before_snap() {
    // Entries arrive out of order: (60,90), (10,30), (30,60).
    // After sort: (10,30), (30,60), (60,90).
    // start=40 → entry starting at 30 → snap to 30.
    // end=75 → entry ending at 90 → snap to 90.
    let es = vec![
        entry(60.0, 90.0, "third"),
        entry(10.0, 30.0, "first"),
        entry(30.0, 60.0, "second"),
    ];
    let r = transcript_refine(40.0, 75.0, Some(&es), Some(300.0));
    let (s, e) = r.expect("should refine even with unsorted input");
    assert!((s - 30.0).abs() < 1e-9, "start snapped to 30 after sort, got {s}");
    assert!((e - 90.0).abs() < 1e-9, "end snapped to 90 after sort, got {e}");
}

#[test]
fn refine_start_in_silence_gap_snaps_back_to_previous_utterance() {
    // FIX 2: entries [10,20) and [50,60) leave a silence gap (20..50).
    // start=35 falls in the gap. Documented rule is "last entry whose
    // start_secs <= start" → that's the [10,20) entry → snap start back to 10.
    // end=55 ∈ [50,60) → end snaps forward to 60.
    let es = entries_vec(&[(10.0, 20.0), (50.0, 60.0)]);
    let r = transcript_refine(35.0, 55.0, Some(&es), Some(300.0));
    let (s, e) = r.expect("should refine across a silence gap");
    assert!((s - 10.0).abs() < 1e-9, "gap start snaps back to prev utterance start (10), got {s}");
    assert!((e - 60.0).abs() < 1e-9, "end snaps forward to 60, got {e}");
}

#[test]
fn refine_overlapping_entries_no_inversion() {
    // Overlapping entries: [0,100) and [50,60). Sorted by start: [(0,100),(50,60)].
    // start=55, end=55.
    // START: last entry whose start_secs <= 55 is [50,60) (start=50) → 50.
    // END: walks sorted order, first entry whose end_secs >= 55 is [0,100)
    //   (end=100) → 100. (end=55 < last.end_secs=60, so the "past last" shortcut
    //   does NOT fire here.) Result [50,100] — non-degenerate.
    let es = vec![entry(0.0, 100.0, "wide"), entry(50.0, 60.0, "inner")];
    let r = transcript_refine(55.0, 55.0, Some(&es), Some(300.0));
    let (s, e) = r.expect("overlapping entries should still produce a usable range");
    // The core guarantee: never inverted / never zero-length regardless of overlap.
    assert!(e > s, "range must be non-degenerate with overlapping entries: [{s}, {e}]");
    assert!((s - 50.0).abs() < 1e-9, "start = last entry starting at/before 55, got {s}");
    assert!((e - 100.0).abs() < 1e-9, "end = first entry ending at/after 55, got {e}");
}

#[test]
fn refine_degenerate_after_snap_returns_none() {
    // Pathological: entry is [50, 51). start=50.5, end=50.6.
    // Both snap to the same entry: start→50, end→51 → not degenerate.
    // Force a degenerate: single entry [50,50] (zero-length).
    let es = vec![TranscriptEntry {
        start_secs: 50.0,
        end_secs: 50.0, // zero-length entry
        speaker: None,
        text: "bad".to_owned(),
        words: None,
    }];
    // After snap: start=50, end=50 → end <= start → None.
    assert_eq!(
        transcript_refine(50.0, 50.5, Some(&es), Some(300.0)),
        None,
        "degenerate snap should return None"
    );
}

#[test]
fn refine_clamps_to_duration() {
    // Entry [290, 350), duration 300 → end clamped to 300.
    let es = vec![entry(290.0, 350.0, "closing")];
    let r = transcript_refine(295.0, 320.0, Some(&es), Some(300.0));
    let (s, e) = r.expect("should refine");
    assert!((s - 290.0).abs() < 1e-9, "start snapped to 290");
    assert!((e - 300.0).abs() < 1e-9, "end clamped to duration 300");
}

#[test]
fn refine_start_exactly_on_entry_boundary_uses_that_entry() {
    // start exactly at the start of entry [60,90). Backward bias:
    // entry starting at 60 → snap to 60.
    let es = entries_vec(&[(30.0, 60.0), (60.0, 90.0)]);
    let r = transcript_refine(60.0, 80.0, Some(&es), Some(300.0));
    let (s, _e) = r.expect("should refine");
    assert!((s - 60.0).abs() < 1e-9, "start on boundary → kept at 60");
}

#[test]
fn refine_end_exactly_on_entry_end_uses_that_end() {
    // end=60 (exactly the end of entry [30,60)). Forward bias: first entry
    // whose end_secs >= 60 is the (30,60) entry → snap end to 60.
    let es = entries_vec(&[(10.0, 30.0), (30.0, 60.0), (60.0, 90.0)]);
    let r = transcript_refine(40.0, 60.0, Some(&es), Some(300.0));
    let (_s, e) = r.expect("should refine");
    assert!((e - 60.0).abs() < 1e-9, "end at boundary → 60");
}

#[test]
fn refine_no_duration_no_clamp() {
    // Without duration, no upper clamp applied.
    let es = vec![entry(10.0, 5000.0, "very long")];
    let r = transcript_refine(100.0, 200.0, Some(&es), None);
    let (s, e) = r.expect("should refine");
    assert!((s - 10.0).abs() < 1e-9, "start snapped to 10");
    assert!((e - 5000.0).abs() < 1e-9, "end snapped to 5000 (no clamp)");
}

// ── handle_auto_snip integration tests (with transcript store) ─────────────────

fn ch(title: &str, start: f64) -> Chapter {
    Chapter::new(title, start)
}

#[test]
fn handle_auto_snip_with_chapters_and_transcript_uses_refined_bounds() {
    // Chapter [60, 120), transcript entry [55, 115).
    // chapter_snap(90, ...) → [60, 120).
    // transcript_refine(60, 120, entries) → start: entry starting at/before 60 → 55;
    //   end: entry whose end >= 120 → 115 (first entry ending at >= 120 is (55,115)? No.
    //   (55,115): end=115 < 120. Next there is no other entry containing 120.
    //   Actually the entry (55,115) has end=115 which is < 120. So 120 > last entry end (115).
    //   Past last entry end → snap to last.end_secs = 115.
    // Final: [55, 115].
    let ep_id = Uuid::new_v4().to_string();
    let chs = vec![ch("Intro", 0.0), ch("Main", 60.0), ch("Outro", 120.0)];
    let ts = vec![entry(55.0, 115.0, "main discussion")];
    let store = build_store_with_transcript(&ep_id, Some(300.0), Some(chs), Some(ts));
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 90.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert_eq!(stored.len(), 1);
    // start snaps back to 55 (entry containing 60 starts at 55).
    assert!((stored[0].start_secs - 55.0).abs() < 1e-9, "start refined to 55");
    // end: 120 > last entry end 115 → snap to 115.
    assert!((stored[0].end_secs - 115.0).abs() < 1e-9, "end refined to 115");
    // chapter title preserved.
    assert_eq!(stored[0].title.as_deref(), Some("Main"));
}

#[test]
fn handle_auto_snip_chapters_no_transcript_uses_chapter_bounds() {
    // No transcript → refinement skipped, chapter bounds used directly.
    let ep_id = Uuid::new_v4().to_string();
    let chs = vec![ch("Intro", 0.0), ch("Main", 60.0), ch("Outro", 120.0)];
    let store = build_store_with_transcript(&ep_id, Some(300.0), Some(chs), None);
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 90.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].start_secs - 60.0).abs() < 1e-9, "chapter start");
    assert!((stored[0].end_secs - 120.0).abs() < 1e-9, "chapter end");
    assert_eq!(stored[0].title.as_deref(), Some("Main"));
}

#[test]
fn handle_auto_snip_no_chapters_no_transcript_falls_back_to_30s() {
    // Neither chapters nor transcript → ±30 s fallback (S2 behaviour unchanged).
    let ep_id = Uuid::new_v4().to_string();
    let store = build_store_with_transcript(&ep_id, Some(300.0), None, None);
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 100.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].start_secs - 70.0).abs() < 1e-9, "fallback start");
    assert!((stored[0].end_secs - 130.0).abs() < 1e-9, "fallback end");
}

#[test]
fn handle_auto_snip_no_chapters_but_has_transcript_refines_30s_range() {
    // No chapters → ±30 s range [70, 130]; transcript entry [65, 135].
    // transcript_refine(70, 130, [(65,135)]):
    //   start: 70 ∈ [65,135) → entry start = 65.
    //   end: 130 ∈ [65,135) → entry end = 135, clamped by duration 300 → 135.
    // Final: [65, 135].
    let ep_id = Uuid::new_v4().to_string();
    let ts = vec![entry(65.0, 135.0, "wide utterance")];
    let store = build_store_with_transcript(&ep_id, Some(300.0), None, Some(ts));
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 100.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].start_secs - 65.0).abs() < 1e-9, "start refined to entry start");
    assert!((stored[0].end_secs - 135.0).abs() < 1e-9, "end refined to entry end");
}

#[test]
fn handle_auto_snip_transcript_degenerate_keeps_chapter_range() {
    // Chapters give [60, 120). Transcript has a zero-length entry at 90.
    // transcript_refine → degenerate → None → keep chapter bounds.
    let ep_id = Uuid::new_v4().to_string();
    let chs = vec![ch("Main", 60.0), ch("Outro", 120.0)];
    let ts = vec![TranscriptEntry {
        start_secs: 90.0,
        end_secs: 90.0, // zero-length
        speaker: None,
        text: "zero".to_owned(),
        words: None,
    }];
    let store = build_store_with_transcript(&ep_id, Some(300.0), Some(chs), Some(ts));
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 90.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    // Degenerate refine → chapter bounds [60, 120) preserved.
    assert!((stored[0].start_secs - 60.0).abs() < 1e-9, "chapter start preserved");
    assert!((stored[0].end_secs - 120.0).abs() < 1e-9, "chapter end preserved");
}

#[test]
fn handle_auto_snip_empty_transcript_entries_keeps_chapter_range() {
    // Chapters give [60, 120). Transcript entries = [].
    // transcript_refine(60, 120, Some(&[])) → None → keep chapter bounds.
    let ep_id = Uuid::new_v4().to_string();
    let chs = vec![ch("Main", 60.0), ch("Outro", 120.0)];
    let store = build_store_with_transcript(&ep_id, Some(300.0), Some(chs), Some(vec![]));
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 90.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].start_secs - 60.0).abs() < 1e-9, "chapter start when empty entries");
    assert!((stored[0].end_secs - 120.0).abs() < 1e-9, "chapter end when empty entries");
}

#[test]
fn handle_auto_snip_transcript_lookup_is_case_insensitive() {
    // FIX 1: transcript stored under one casing, autosnip id arrives in a
    // different casing. The READ path must match case-insensitively (as robust
    // as the episode/chapter lookup) → refinement still applies. Before the
    // fix, the exact-then-lowercase QUERY missed when the stored key was
    // UPPERCASE, silently falling back to chapter bounds.
    let ep_id = Uuid::new_v4().to_string(); // lowercase UUID string
    let ep_id_upper = ep_id.to_uppercase();
    let chs = vec![ch("Main", 60.0), ch("Outro", 120.0)];
    let ts = vec![entry(55.0, 115.0, "main discussion")];

    // Store the episode by the canonical (lowercase) id, but key the timed
    // transcript under the UPPERCASE form — mimicking an iOS report casing skew.
    let store = build_store_with_transcript(&ep_id, Some(300.0), Some(chs), None);
    store
        .lock()
        .unwrap()
        .set_timed_transcript(ep_id_upper, ts);

    let (h, clips, _rev) = fresh_handler(store);
    // Autosnip id arrives lowercase; transcript key is UPPERCASE.
    let v = h.handle_auto_snip(ep_id, 90.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    // Transcript-refined bounds, NOT chapter bounds [60,120):
    //   start: entry starting at/before 60 → 55.
    //   end: 120 > last entry end 115 → snap to 115.
    assert!(
        (stored[0].start_secs - 55.0).abs() < 1e-9,
        "case-insensitive transcript match should refine start to 55, got {}",
        stored[0].start_secs
    );
    assert!(
        (stored[0].end_secs - 115.0).abs() < 1e-9,
        "case-insensitive transcript match should refine end to 115, got {}",
        stored[0].end_secs
    );
}
