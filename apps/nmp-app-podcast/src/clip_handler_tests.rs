//! Tests for [`super::clip_handler`] — create/delete/auto-snip and projection coverage.
//!
//! Extracted from `clip_handler.rs` to keep that file under the 500-line hard limit.

use super::*;
use podcast_core::{Chapter, Episode, EpisodeId, Podcast};
use url::Url;
use uuid::Uuid;

fn fresh_store_with_episode(ep_id: &str, duration: Option<f64>) -> Arc<Mutex<PodcastStore>> {
    let mut podcast = Podcast::new("Some Show");
    podcast.feed_url = Some(Url::parse("https://ex.com/rss").unwrap());
    let mut episode = Episode::new(
        podcast.id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        "Pilot",
        Url::parse("https://ex.com/ep-1.mp3").unwrap(),
        Utc::now(),
    );
    episode.id = EpisodeId(Uuid::parse_str(ep_id).unwrap());
    episode.duration_secs = duration;
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    store.lock().unwrap().subscribe(podcast, vec![episode]);
    store
}

fn fresh_handler(
    store: Arc<Mutex<PodcastStore>>,
) -> (ClipHandler, Arc<Mutex<Vec<ClipRecord>>>, Arc<AtomicU64>) {
    let clips = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(AtomicU64::new(0));
    let h = ClipHandler::new(clips.clone(), store, rev.clone());
    (h, clips, rev)
}

fn timed_entry(
    start_secs: f64,
    end_secs: f64,
    text: &str,
    speaker: &str,
) -> podcast_transcripts::TranscriptEntry {
    podcast_transcripts::TranscriptEntry {
        start_secs,
        end_secs,
        text: text.to_owned(),
        speaker: Some(speaker.to_owned()),
        words: None,
    }
}

#[test]
fn create_rejects_unknown_episode() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (h, clips, rev) = fresh_handler(store);
    let v = h.handle_create("ghost".into(), 1.0, 5.0, None, None, None, None);
    assert_eq!(v["ok"], false);
    assert!(clips.lock().unwrap().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn create_rejects_inverted_range() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(300.0));
    let (h, clips, _rev) = fresh_handler(store);
    // start == end → 0-length, rejected.
    let v = h.handle_create(ep_id.clone(), 10.0, 10.0, None, None, None, None);
    assert_eq!(v["ok"], false);
    assert!(clips.lock().unwrap().is_empty());
}

#[test]
fn create_swaps_inverted_inputs_into_valid_range() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(300.0));
    let (h, clips, rev) = fresh_handler(store);
    // start > end → normalize, then accept.
    let v = h.handle_create(
        ep_id.clone(),
        70.0,
        10.0,
        Some("flipped".into()),
        None,
        None,
        None,
    );
    assert_eq!(v["ok"], true);
    assert!(v["clip_id"].is_string());
    let stored = clips.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].start_secs, 10.0);
    assert_eq!(stored[0].end_secs, 70.0);
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn delete_removes_existing_clip_and_bumps_rev() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(300.0));
    let (h, clips, rev) = fresh_handler(store);
    let create = h.handle_create(ep_id, 5.0, 25.0, None, None, None, None);
    let clip_id = create["clip_id"].as_str().unwrap().to_owned();
    rev.store(0, Ordering::Relaxed);
    let v = h.handle_delete(clip_id);
    assert_eq!(v["ok"], true);
    assert!(clips.lock().unwrap().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn delete_unknown_clip_is_ok_but_does_not_bump_rev() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (h, _clips, rev) = fresh_handler(store);
    let v = h.handle_delete("nope".into());
    assert_eq!(v["ok"], true);
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn auto_snip_without_transcript_uses_pending_plus_minus_30_window() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(300.0));
    let (h, clips, _rev) = fresh_handler(store);
    let v = h.handle_auto_snip(ep_id, 100.0, None, None);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert!((stored[0].start_secs - 70.0).abs() < 1e-9);
    assert!((stored[0].end_secs - 130.0).abs() < 1e-9);
    assert_eq!(stored[0].refinement_status, "pending_transcript");
}

#[test]
fn auto_snip_clamps_to_episode_bounds() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(40.0));
    let (h, clips, _rev) = fresh_handler(store);
    // Near the start — start should clamp to 0.
    let v = h.handle_auto_snip(ep_id.clone(), 5.0, None, None);
    assert_eq!(v["ok"], true);
    // Near the end — end should clamp to duration (40.0).
    let _ = h.handle_auto_snip(ep_id, 35.0, None, None);
    let stored = clips.lock().unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].start_secs, 0.0);
    // Second clip: end clamps to 40.0.
    assert!((stored[1].end_secs - 40.0).abs() < 1e-9);
}

#[test]
fn auto_snip_without_known_duration_does_not_clamp_end() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, None);
    let (h, clips, _rev) = fresh_handler(store);
    let v = h.handle_auto_snip(ep_id, 100.0, None, None);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].end_secs - 130.0).abs() < 1e-9);
}

#[test]
fn auto_snip_refines_to_transcript_boundaries_when_entries_exist() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(300.0));
    store.lock().unwrap().set_timed_transcript(
        ep_id.clone(),
        vec![
            timed_entry(20.0, 35.0, "Setup.", "spk_0"),
            timed_entry(35.0, 50.0, "Important bit.", "spk_0"),
            timed_entry(50.0, 65.0, "Conclusion.", "spk_0"),
        ],
    );
    let (h, clips, _rev) = fresh_handler(store);
    let v = h.handle_auto_snip(ep_id, 55.0, Some("headphone".to_owned()), None);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert_eq!(stored[0].start_secs, 20.0);
    assert_eq!(stored[0].end_secs, 65.0);
    assert_eq!(
        stored[0].transcript_text,
        "Setup. Important bit. Conclusion."
    );
    assert_eq!(stored[0].speaker.as_deref(), Some("spk_0"));
    assert_eq!(stored[0].source, "headphone");
    assert_eq!(stored[0].refinement_status, "transcript_refined");
}

#[test]
fn pending_auto_snip_refines_when_transcript_arrives() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(300.0));
    let (h, clips, rev) = fresh_handler(store.clone());
    let v = h.handle_auto_snip(ep_id.clone(), 55.0, None, None);
    assert_eq!(v["ok"], true);
    assert_eq!(clips.lock().unwrap()[0].refinement_status, "pending_transcript");
    rev.store(0, Ordering::Relaxed);
    store.lock().unwrap().set_timed_transcript(
        ep_id.clone(),
        vec![
            timed_entry(25.0, 40.0, "Before.", "spk_0"),
            timed_entry(40.0, 60.0, "Anchor.", "spk_0"),
        ],
    );
    h.refine_pending_for_episode(&ep_id);
    let stored = clips.lock().unwrap();
    assert_eq!(stored[0].start_secs, 25.0);
    assert_eq!(stored[0].end_secs, 60.0);
    assert_eq!(stored[0].transcript_text, "Before. Anchor.");
    assert_eq!(stored[0].refinement_status, "transcript_refined");
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

fn library_with_show(ep_id: &str, episode_title: &str, show_title: &str) -> Vec<PodcastSummary> {
    use crate::ffi::projections::EpisodeSummary;
    vec![PodcastSummary {
        id: Uuid::new_v4().to_string(),
        title: show_title.into(),
        episode_count: 1,
        unplayed_count: 0,
        artwork_url: None,
        feed_url: None,
        author: None,
        description: None,
        last_refreshed_at: None,
        title_is_placeholder: false,
        is_subscribed: true,
        owner_pubkey_hex: None,
        nostr_visibility: "public".into(),
        auto_download: false,
        auto_download_mode: String::new(),
        auto_download_count: 0,
        cellular_allowed: false,
        notifications_enabled: true,
        user_categories: Vec::new(),
        transcription_enabled: true,
        episodes: vec![EpisodeSummary {
            id: ep_id.into(),
            title: episode_title.into(),
            podcast_id: None,
            podcast_title: Some(show_title.into()),
            ..EpisodeSummary::default()
        }],
    }]
}

#[test]
fn project_clips_picks_up_renamed_titles_from_live_library() {
    // Clip captured with stale titles ("Old Show" / "Old Episode") still in
    // ClipRecord; library now reports new ones. Projection prefers the
    // live names.
    let ep_id = Uuid::new_v4().to_string();
    let clips = Arc::new(Mutex::new(vec![ClipRecord {
        id: "clip-1".into(),
        episode_id: ep_id.clone(),
        episode_title: "Old Episode".into(),
        podcast_title: "Old Show".into(),
        start_secs: 0.0,
        end_secs: 10.0,
        title: None,
        transcript_text: String::new(),
        speaker: None,
        source: "touch".to_owned(),
        refinement_status: "manual".to_owned(),
        auto_snip_anchor_secs: None,
        created_at: 1,
    }]));
    let library = library_with_show(&ep_id, "Fresh Episode", "Fresh Show");
    let projected = project_clips(&clips, &library);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].episode_title, "Fresh Episode");
    assert_eq!(projected[0].podcast_title, "Fresh Show");
}

#[test]
fn project_clips_falls_back_to_frozen_titles_when_episode_missing() {
    // Episode no longer in the library (unsubscribed) — projection
    // surfaces the create-time titles so the row still renders.
    let clips = Arc::new(Mutex::new(vec![ClipRecord {
        id: "clip-1".into(),
        episode_id: "ghost-ep".into(),
        episode_title: "Frozen Episode".into(),
        podcast_title: "Frozen Show".into(),
        start_secs: 0.0,
        end_secs: 10.0,
        title: None,
        transcript_text: String::new(),
        speaker: None,
        source: "touch".to_owned(),
        refinement_status: "manual".to_owned(),
        auto_snip_anchor_secs: None,
        created_at: 1,
    }]));
    let projected = project_clips(&clips, &[]);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].episode_title, "Frozen Episode");
    assert_eq!(projected[0].podcast_title, "Frozen Show");
}

// ── Persistence integration tests ─────────────────────────────────────────

/// Minimal RAII temp dir for tests that need a data directory.
struct TempDir {
    path: std::path::PathBuf,
}
impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("clip-handler-test-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Build a store that has a data dir and one subscribable episode.
fn store_with_dir_and_episode(dir: &TempDir) -> (Arc<Mutex<PodcastStore>>, String) {
    let ep_id = Uuid::new_v4().to_string();
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    {
        let mut s = store.lock().unwrap();
        s.set_data_dir(dir.path.clone());
        let podcast = Podcast::new("Persist Show");
        let episode = Episode::new(
            podcast.id,
            "https://example.com/feed.xml",
            format!("guid-{}", Uuid::new_v4()),
            "Persist Episode",
            Url::parse("https://example.com/ep.mp3").unwrap(),
            Utc::now(),
        );
        // Override the episode id to a known UUID string.
        let mut ep = episode;
        ep.id = EpisodeId(Uuid::parse_str(&ep_id).unwrap());
        s.subscribe(podcast, vec![ep]);
    }
    (store, ep_id)
}

#[test]
fn create_clip_persists_survives_restart() {
    // Round-trip: create a clip → reload store from the same dir → clip present.
    let dir = TempDir::new();
    let (store, ep_id) = store_with_dir_and_episode(&dir);
    let clips_arc = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let h = ClipHandler::new(clips_arc.clone(), store.clone(), rev.clone());

    let out = h.handle_create(ep_id.clone(), 10.0, 40.0, Some("keep me".into()));
    assert_eq!(out["ok"], true, "create must succeed");
    let clip_id = out["clip_id"].as_str().unwrap().to_owned();

    // Simulate restart: fresh store from same data dir.
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    let reloaded = store2.clips().to_vec();
    assert_eq!(reloaded.len(), 1, "clip must survive restart");
    assert_eq!(reloaded[0].id, clip_id);
    assert_eq!(reloaded[0].episode_id, ep_id);
    assert_eq!(reloaded[0].start_secs, 10.0);
    assert_eq!(reloaded[0].end_secs, 40.0);
    assert_eq!(reloaded[0].title, Some("keep me".to_owned()));
}

#[test]
fn delete_clip_persists_survives_restart() {
    // Create two clips, delete one, reload — only the remaining one is present.
    let dir = TempDir::new();
    let (store, ep_id) = store_with_dir_and_episode(&dir);
    let clips_arc = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let h = ClipHandler::new(clips_arc.clone(), store.clone(), rev.clone());

    let out1 = h.handle_create(ep_id.clone(), 0.0, 30.0, Some("first".into()));
    let out2 = h.handle_create(ep_id.clone(), 30.0, 60.0, Some("second".into()));
    let id1 = out1["clip_id"].as_str().unwrap().to_owned();
    let id2 = out2["clip_id"].as_str().unwrap().to_owned();

    // Delete the first clip.
    let del = h.handle_delete(id1.clone());
    assert_eq!(del["ok"], true);

    // Simulate restart.
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    let reloaded = store2.clips().to_vec();
    assert_eq!(reloaded.len(), 1, "only second clip survives");
    assert_eq!(reloaded[0].id, id2);
    assert_eq!(reloaded[0].title, Some("second".to_owned()));
}

#[test]
fn project_clips_returns_newest_first() {
    let clips = Arc::new(Mutex::new(vec![
        ClipRecord {
            id: "older".into(),
            episode_id: "ep".into(),
            episode_title: "Ep".into(),
            podcast_title: "Show".into(),
            start_secs: 0.0,
            end_secs: 10.0,
            title: None,
            transcript_text: String::new(),
            speaker: None,
            source: "touch".to_owned(),
            refinement_status: "manual".to_owned(),
            auto_snip_anchor_secs: None,
            created_at: 1,
        },
        ClipRecord {
            id: "newer".into(),
            episode_id: "ep".into(),
            episode_title: "Ep".into(),
            podcast_title: "Show".into(),
            start_secs: 0.0,
            end_secs: 10.0,
            title: None,
            transcript_text: String::new(),
            speaker: None,
            source: "touch".to_owned(),
            refinement_status: "manual".to_owned(),
            auto_snip_anchor_secs: None,
            created_at: 2,
        },
    ]));
    let projected = project_clips(&clips, &[]);
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].id, "newer");
    assert_eq!(projected[1].id, "older");
}

// ── chapter_snap unit tests ───────────────────────────────────────────────────

/// Build a simple chapter with only a title and start_secs.
fn ch(title: &str, start: f64) -> Chapter {
    Chapter::new(title, start)
}

#[test]
fn chapter_snap_no_chapters_falls_back_to_30s_window() {
    // None → fallback.
    let (s, e, t) = chapter_snap(100.0, None, Some(300.0));
    assert!((s - 70.0).abs() < 1e-9, "start should be pos-30");
    assert!((e - 130.0).abs() < 1e-9, "end should be pos+30");
    assert!(t.is_none());
}

#[test]
fn chapter_snap_empty_chapters_falls_back_to_30s_window() {
    // Some(vec![]) → same fallback as None.
    let (s, e, t) = chapter_snap(100.0, Some(&[]), Some(300.0));
    assert!((s - 70.0).abs() < 1e-9);
    assert!((e - 130.0).abs() < 1e-9);
    assert!(t.is_none());
}

#[test]
fn chapter_snap_pos_inside_chapter_2() {
    // Three chapters: 0–60, 60–120, 120–end(300).
    let chs = vec![ch("Intro", 0.0), ch("Main", 60.0), ch("Outro", 120.0)];
    // pos = 90.0 → inside chapter "Main" [60, 120).
    let (s, e, title) = chapter_snap(90.0, Some(&chs), Some(300.0));
    assert!((s - 60.0).abs() < 1e-9, "start = ch2.start");
    assert!((e - 120.0).abs() < 1e-9, "end = ch3.start");
    assert_eq!(title.as_deref(), Some("Main"));
}

#[test]
fn chapter_snap_pos_inside_last_chapter() {
    // Three chapters; pos in the last one → end = duration.
    let chs = vec![ch("Intro", 0.0), ch("Main", 60.0), ch("Outro", 120.0)];
    let (s, e, title) = chapter_snap(200.0, Some(&chs), Some(300.0));
    assert!((s - 120.0).abs() < 1e-9, "start = last chapter start");
    assert!((e - 300.0).abs() < 1e-9, "end clamped to duration");
    assert_eq!(title.as_deref(), Some("Outro"));
}

#[test]
fn chapter_snap_pos_before_first_chapter() {
    // Chapters start at 10 s; pos = 3 s → pre-chapter segment [0, 10].
    let chs = vec![ch("Act I", 10.0), ch("Act II", 60.0)];
    let (s, e, title) = chapter_snap(3.0, Some(&chs), Some(300.0));
    assert!((s - 0.0).abs() < 1e-9, "start = 0");
    assert!((e - 10.0).abs() < 1e-9, "end = first chapter start");
    assert!(title.is_none(), "no chapter title for pre-chapter segment");
}

#[test]
fn chapter_snap_pos_past_duration_clamped() {
    // pos past duration → last chapter, end clamped to duration.
    let chs = vec![ch("Only", 0.0), ch("Final", 200.0)];
    let (s, e, _) = chapter_snap(400.0, Some(&chs), Some(300.0));
    assert!((s - 200.0).abs() < 1e-9);
    assert!((e - 300.0).abs() < 1e-9);
}

#[test]
fn chapter_snap_single_chapter_snaps_to_full_duration() {
    let chs = vec![ch("Solo", 0.0)];
    let (s, e, title) = chapter_snap(50.0, Some(&chs), Some(120.0));
    assert!((s - 0.0).abs() < 1e-9);
    assert!((e - 120.0).abs() < 1e-9, "end = duration (no next chapter)");
    assert_eq!(title.as_deref(), Some("Solo"));
}

#[test]
fn chapter_snap_fallback_clamps_near_start() {
    // No chapters, pos near start → start clamps to 0.
    let (s, e, _) = chapter_snap(5.0, None, Some(300.0));
    assert!((s - 0.0).abs() < 1e-9);
    assert!((e - 35.0).abs() < 1e-9);
}

#[test]
fn chapter_snap_fallback_clamps_near_end() {
    // No chapters, pos near end → end clamps to duration.
    let (s, e, _) = chapter_snap(290.0, None, Some(300.0));
    assert!((s - 260.0).abs() < 1e-9);
    assert!((e - 300.0).abs() < 1e-9);
}

#[test]
fn chapter_snap_sorts_unsorted_input_then_snaps() {
    // Chapters arrive out of order: starts [120, 0, 60]. pos = 90 must still
    // snap to the "Main" chapter [60, 120) — proving the internal sort runs.
    let chs = vec![ch("Outro", 120.0), ch("Intro", 0.0), ch("Main", 60.0)];
    let (s, e, title) = chapter_snap(90.0, Some(&chs), Some(300.0));
    assert!((s - 60.0).abs() < 1e-9, "start = sorted ch2.start");
    assert!((e - 120.0).abs() < 1e-9, "end = sorted ch3.start");
    assert_eq!(title.as_deref(), Some("Main"));
}

#[test]
fn chapter_snap_duplicate_start_chapters_no_inverted_range() {
    // Two chapters share start 60.0: starts [0, 60, 60, 120]. pos = 70 lands
    // in the 60-region. Result must be deterministic and non-degenerate
    // (end > start) — never an inverted/zero range, never a panic.
    let chs = vec![
        ch("Intro", 0.0),
        ch("Main A", 60.0),
        ch("Main B", 60.0),
        ch("Outro", 120.0),
    ];
    let (s, e, title) = chapter_snap(70.0, Some(&chs), Some(300.0));
    assert!(e > s, "range must be non-degenerate: got [{s}, {e}]");
    // Stable sort keeps "Main A" before "Main B"; the first 60-start chapter
    // has a zero-width interval [60, 60) (next is also 60), so the resolver
    // advances to "Main B" whose interval [60, 120) actually contains pos=70.
    assert!((s - 60.0).abs() < 1e-9, "start snaps to the 60 s boundary");
    assert!((e - 120.0).abs() < 1e-9, "end = next distinct boundary (120)");
    assert_eq!(title.as_deref(), Some("Main B"));
}

#[test]
fn chapter_snap_pos_exactly_on_boundary_belongs_to_starting_chapter() {
    // Half-open [start, next) semantics: pos exactly == a chapter.start
    // belongs to the chapter that STARTS at it, not the previous one.
    let chs = vec![ch("Intro", 0.0), ch("Main", 60.0), ch("Outro", 120.0)];
    let (s, e, title) = chapter_snap(60.0, Some(&chs), Some(300.0));
    assert!((s - 60.0).abs() < 1e-9, "pos==60 → owned by 'Main' (starts at 60)");
    assert!((e - 120.0).abs() < 1e-9);
    assert_eq!(title.as_deref(), Some("Main"));
}

#[test]
fn chapter_snap_last_chapter_at_duration_falls_back_to_30s() {
    // FIX 1: last chapter starts exactly at duration → chapter range would be
    // [300, 300] (degenerate), which handle_create rejects. The resolver must
    // fall back to the ±30 s clamped window so AutoSnip still produces a
    // usable clip (end > start).
    let chs = vec![ch("Intro", 0.0), ch("End Marker", 300.0)];
    let (s, e, title) = chapter_snap(300.0, Some(&chs), Some(300.0));
    assert!(e > s, "must be usable, not degenerate: got [{s}, {e}]");
    // ±30 s window around pos=300 clamped to duration=300 → [270, 300].
    assert!((s - 270.0).abs() < 1e-9, "fallback start = pos-30");
    assert!((e - 300.0).abs() < 1e-9, "fallback end clamped to duration");
    assert!(title.is_none(), "fallback path carries no chapter title");
}

// ── handler-level chapter snap tests (store integration) ─────────────────────

fn fresh_store_with_episode_and_chapters(
    ep_id: &str,
    duration: Option<f64>,
    chapters: Option<Vec<Chapter>>,
) -> Arc<Mutex<PodcastStore>> {
    let mut podcast = Podcast::new("Chapter Show");
    podcast.feed_url = Some(Url::parse("https://ex.com/rss").unwrap());
    let mut episode = Episode::new(
        podcast.id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        "Chapter Episode",
        Url::parse("https://ex.com/ep.mp3").unwrap(),
        Utc::now(),
    );
    episode.id = EpisodeId(Uuid::parse_str(ep_id).unwrap());
    episode.duration_secs = duration;
    episode.chapters = chapters;
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    store.lock().unwrap().subscribe(podcast, vec![episode]);
    store
}

#[test]
fn auto_snip_chapter_episode_snaps_to_chapter_boundaries() {
    // Two chapters: [0, 60) and [60, 300).
    // pos = 90 → clip should be [60, 120] (not ±30 s).
    // Wait — three chapters: 0, 60, 120 → pos 90 snaps to [60, 120].
    let ep_id = Uuid::new_v4().to_string();
    let chs = vec![ch("Intro", 0.0), ch("Main", 60.0), ch("Outro", 120.0)];
    let store = fresh_store_with_episode_and_chapters(&ep_id, Some(300.0), Some(chs));
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 90.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert!((stored[0].start_secs - 60.0).abs() < 1e-9, "start snaps to ch2");
    assert!((stored[0].end_secs - 120.0).abs() < 1e-9, "end = ch3 start");
    assert_eq!(stored[0].title.as_deref(), Some("Main"), "chapter title used");
}

#[test]
fn auto_snip_chapter_last_chapter_uses_duration() {
    let ep_id = Uuid::new_v4().to_string();
    let chs = vec![ch("Intro", 0.0), ch("Outro", 120.0)];
    let store = fresh_store_with_episode_and_chapters(&ep_id, Some(300.0), Some(chs));
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 200.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].start_secs - 120.0).abs() < 1e-9);
    assert!((stored[0].end_secs - 300.0).abs() < 1e-9);
}

#[test]
fn auto_snip_no_chapters_field_falls_back_to_30s() {
    // chapters field is None — old ±30 s path unchanged.
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode_and_chapters(&ep_id, Some(300.0), None);
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 100.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].start_secs - 70.0).abs() < 1e-9);
    assert!((stored[0].end_secs - 130.0).abs() < 1e-9);
}

#[test]
fn auto_snip_empty_chapters_vec_falls_back_to_30s() {
    // chapters = Some(vec![]) → still falls back.
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode_and_chapters(&ep_id, Some(300.0), Some(vec![]));
    let (h, clips, _rev) = fresh_handler(store);

    let v = h.handle_auto_snip(ep_id, 100.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].start_secs - 70.0).abs() < 1e-9);
    assert!((stored[0].end_secs - 130.0).abs() < 1e-9);
}

// ── transcript_refine tests (S3a) — split into their own file ─────────────────
#[cfg(test)]
#[path = "clip_handler_transcript_tests.rs"]
mod transcript_tests;
