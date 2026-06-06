//! Tests for [`super::clip_handler`] — create/delete/auto-snip and projection coverage.
//!
//! Extracted from `clip_handler.rs` to keep that file under the 500-line hard limit.

use super::*;
use podcast_core::{Episode, EpisodeId, Podcast};
use url::Url;

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

#[test]
fn create_rejects_unknown_episode() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (h, clips, rev) = fresh_handler(store);
    let v = h.handle_create("ghost".into(), 1.0, 5.0, None);
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
    let v = h.handle_create(ep_id.clone(), 10.0, 10.0, None);
    assert_eq!(v["ok"], false);
    assert!(clips.lock().unwrap().is_empty());
}

#[test]
fn create_swaps_inverted_inputs_into_valid_range() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(300.0));
    let (h, clips, rev) = fresh_handler(store);
    // start > end → normalize, then accept.
    let v = h.handle_create(ep_id.clone(), 70.0, 10.0, Some("flipped".into()));
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
    let create = h.handle_create(ep_id, 5.0, 25.0, None);
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
fn auto_snip_uses_plus_minus_30_window() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(300.0));
    let (h, clips, _rev) = fresh_handler(store);
    let v = h.handle_auto_snip(ep_id, 100.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert!((stored[0].start_secs - 70.0).abs() < 1e-9);
    assert!((stored[0].end_secs - 130.0).abs() < 1e-9);
}

#[test]
fn auto_snip_clamps_to_episode_bounds() {
    let ep_id = Uuid::new_v4().to_string();
    let store = fresh_store_with_episode(&ep_id, Some(40.0));
    let (h, clips, _rev) = fresh_handler(store);
    // Near the start — start should clamp to 0.
    let v = h.handle_auto_snip(ep_id.clone(), 5.0);
    assert_eq!(v["ok"], true);
    // Near the end — end should clamp to duration (40.0).
    let _ = h.handle_auto_snip(ep_id, 35.0);
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
    let v = h.handle_auto_snip(ep_id, 100.0);
    assert_eq!(v["ok"], true);
    let stored = clips.lock().unwrap();
    assert!((stored[0].end_secs - 130.0).abs() < 1e-9);
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
        cellular_allowed: false,
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
        created_at: 1,
    }]));
    let projected = project_clips(&clips, &[]);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].episode_title, "Frozen Episode");
    assert_eq!(projected[0].podcast_title, "Frozen Show");
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
            created_at: 2,
        },
    ]));
    let projected = project_clips(&clips, &[]);
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].id, "newer");
    assert_eq!(projected[1].id, "older");
}
