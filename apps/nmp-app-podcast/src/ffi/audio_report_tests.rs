//! Tests for [`super::audio_report`] — playback-position writeback and flush-throttle coverage.
//!
//! Extracted from `audio_report.rs` to keep that file under the 500-line hard limit.

use super::*;
use podcast_core::{Episode, Podcast, PodcastId};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

/// RAII tempdir local to this module so the writeback tests are
/// self-contained and don't pull in `tempfile`.
struct TempDir {
    path: PathBuf,
}
impl TempDir {
    fn new(label: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nmp-audio-report-{}-{}-{}",
            label,
            std::process::id(),
            n
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn make_episode(podcast_id: PodcastId, title: &str) -> Episode {
    let guid = format!("guid-{}", Uuid::new_v4());
    Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        guid,
        title,
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    )
}

#[test]
fn playing_report_writes_position_back_to_store() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Resume Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Ep");
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    let report = AudioReport::Playing {
        url: "https://example.com/audio.mp3".into(),
        position_secs: 17.0,
        duration_secs: 1800.0,
    };
    apply_writeback(&mut store, &report, &ep_id);

    assert_eq!(store.position_for(&ep_id), Some(17.0));
}

#[test]
fn paused_report_flushes_to_disk() {
    let dir = TempDir::new("paused");
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = Podcast::new("Pause Flush");
    let pid = podcast.id;
    let ep = make_episode(pid, "Ep");
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    let report = AudioReport::Paused {
        url: "https://example.com/audio.mp3".into(),
        position_secs: 42.0,
    };
    apply_writeback(&mut store, &report, &ep_id);

    let mut reloaded = PodcastStore::new();
    reloaded.set_data_dir(dir.path.clone());
    assert_eq!(reloaded.position_for(&ep_id), Some(42.0));
}

#[test]
fn playing_ticks_only_flush_after_position_delta() {
    let dir = TempDir::new("throttle");
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = Podcast::new("Throttle");
    let pid = podcast.id;
    let ep = make_episode(pid, "Ep");
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    // Two close ticks — neither crosses the delta, so the on-disk file
    // should still report position 0 after reload.
    apply_writeback(
        &mut store,
        &AudioReport::Playing {
            url: "u".into(),
            position_secs: 5.0,
            duration_secs: 600.0,
        },
        &ep_id,
    );
    apply_writeback(
        &mut store,
        &AudioReport::Playing {
            url: "u".into(),
            position_secs: 10.0,
            duration_secs: 600.0,
        },
        &ep_id,
    );
    let mut reloaded_before = PodcastStore::new();
    reloaded_before.set_data_dir(dir.path.clone());
    assert_eq!(reloaded_before.position_for(&ep_id), None);

    // A tick that crosses the 30 s delta triggers a flush.
    apply_writeback(
        &mut store,
        &AudioReport::Playing {
            url: "u".into(),
            position_secs: 45.0,
            duration_secs: 600.0,
        },
        &ep_id,
    );
    let mut reloaded_after = PodcastStore::new();
    reloaded_after.set_data_dir(dir.path.clone());
    assert_eq!(reloaded_after.position_for(&ep_id), Some(45.0));
}

#[test]
fn unknown_episode_id_is_a_noop() {
    let mut store = PodcastStore::new();
    // Empty store → `set_episode_position` returns false → no flush,
    // no panic, no disk-touch attempt.
    apply_writeback(
        &mut store,
        &AudioReport::Playing {
            url: "u".into(),
            position_secs: 1.0,
            duration_secs: 60.0,
        },
        "no-such-episode",
    );
}

#[test]
fn failed_and_buffering_reports_do_not_mutate_position() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Inert Reports");
    let pid = podcast.id;
    let mut ep = make_episode(pid, "Ep");
    ep.position_secs = 12.0;
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    apply_writeback(
        &mut store,
        &AudioReport::BufferingProgress { fraction: 0.5 },
        &ep_id,
    );
    apply_writeback(
        &mut store,
        &AudioReport::Failed {
            url: "u".into(),
            error: "boom".into(),
        },
        &ep_id,
    );
    assert_eq!(store.position_for(&ep_id), Some(12.0));
}

/// Regression for the throttling bug: 200 small ≤4 Hz ticks (typical of a
/// real playback stream, each advancing ~0.25 s) must still produce at
/// least one mid-stream flush so a hard kill loses at most one delta of
/// position. The earlier `prev = position_for(...)` comparison made this
/// loop never flush — the fix anchors the throttle to the last
/// **flushed** position instead.
#[test]
fn continuous_playback_checkpoints_periodically() {
    let dir = TempDir::new("continuous");
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = Podcast::new("Continuous");
    let pid = podcast.id;
    let ep = make_episode(pid, "Ep");
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    // 200 ticks at 0.25 s each = 50 s of playback. At a 30 s flush
    // threshold the stream should checkpoint at least once mid-stream.
    for i in 1..=200 {
        apply_writeback(
            &mut store,
            &AudioReport::Playing {
                url: "u".into(),
                position_secs: (i as f64) * 0.25,
                duration_secs: 3600.0,
            },
            &ep_id,
        );
    }

    // Reload from disk without flushing — the on-disk position must be
    // past the first 30 s threshold (so a kill mid-stream loses at most
    // ~30 s, not the entire 50 s).
    let mut reloaded = PodcastStore::new();
    reloaded.set_data_dir(dir.path.clone());
    let on_disk = reloaded.position_for(&ep_id).expect("checkpointed");
    assert!(
        on_disk >= 30.0,
        "expected an on-disk checkpoint past 30 s, got {on_disk}"
    );
}

#[test]
fn item_end_marks_episode_played_and_flushes() {
    let dir = TempDir::new("item-end");
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = Podcast::new("Finish Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Episode");
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    // Simulate a Playing tick just before the end.
    apply_writeback(
        &mut store,
        &AudioReport::Playing {
            url: "u".into(),
            position_secs: 59.5,
            duration_secs: 60.0,
        },
        &ep_id,
    );

    // ItemEnd fires: episode must be marked played.
    apply_writeback(
        &mut store,
        &AudioReport::ItemEnd { url: "u".into() },
        &ep_id,
    );

    // Verify in-memory state.
    let played_in_memory = store
        .all_podcasts()
        .iter()
        .flat_map(|(_, eps)| eps.iter())
        .find(|e| e.id.0.to_string() == ep_id)
        .map(|e| e.played)
        .expect("episode present");
    assert!(
        played_in_memory,
        "episode must be marked played after ItemEnd"
    );

    // Verify the played flag survives a reload from disk.
    let mut reloaded = PodcastStore::new();
    reloaded.set_data_dir(dir.path.clone());
    let played_on_disk = reloaded
        .all_podcasts()
        .iter()
        .flat_map(|(_, eps)| eps.iter())
        .find(|e| e.id.0.to_string() == ep_id)
        .map(|e| e.played)
        .expect("episode present after reload");
    assert!(played_on_disk, "played flag must persist across restart");
}

#[test]
fn item_end_deletes_download_when_auto_delete_on() {
    // Seam test: the ItemEnd writeback branch must honour the kernel-owned
    // delete-after-played policy. With auto-mark + auto-delete both on, a
    // downloaded episode's local file is removed from disk on natural end.
    let dir = TempDir::new("item-end-autodelete");
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = Podcast::new("Auto-delete Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Episode");
    let typed_id = ep.id;
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    // Write a real file and register it as the episode's download so the
    // `remove_file` leg has something to remove.
    let audio_path = dir.path.join("episode.mp3");
    std::fs::write(&audio_path, b"audio bytes").expect("write fixture file");
    store.set_local_path(typed_id, audio_path.to_string_lossy().into_owned(), 11);
    store.set_auto_mark_played_at_end(true);
    store.set_auto_delete_downloads_after_played(true);
    assert!(audio_path.exists());

    apply_writeback(
        &mut store,
        &AudioReport::ItemEnd { url: "u".into() },
        &ep_id,
    );

    assert!(
        store.local_path_for(&typed_id).is_none(),
        "local-path mapping must be cleared after ItemEnd with auto-delete on"
    );
    assert!(
        !audio_path.exists(),
        "the downloaded file must be removed from disk"
    );
}

#[test]
fn item_end_keeps_download_when_auto_delete_off() {
    // With auto-delete OFF, ItemEnd marks played but the local download (and
    // file) survive.
    let dir = TempDir::new("item-end-keep");
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = Podcast::new("Keep Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Episode");
    let typed_id = ep.id;
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    let audio_path = dir.path.join("episode.mp3");
    std::fs::write(&audio_path, b"audio bytes").expect("write fixture file");
    store.set_local_path(typed_id, audio_path.to_string_lossy().into_owned(), 11);
    store.set_auto_mark_played_at_end(true);
    // auto_delete_downloads_after_played defaults to false.

    apply_writeback(
        &mut store,
        &AudioReport::ItemEnd { url: "u".into() },
        &ep_id,
    );

    assert!(
        store.local_path_for(&typed_id).is_some(),
        "local-path mapping must survive when auto-delete is off"
    );
    assert!(
        audio_path.exists(),
        "the downloaded file must remain on disk"
    );
}

#[test]
fn item_end_rewinds_position_to_zero() {
    // A natural play-to-completion must reset the stored position to 0 so the
    // next play starts from the beginning instead of resuming at the end.
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Rewind Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Episode");
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    // Engine emits a Playing tick near the end, then a Paused at duration, then
    // ItemEnd — mirroring the real report sequence the regression came from.
    apply_writeback(
        &mut store,
        &AudioReport::Playing {
            url: "u".into(),
            position_secs: 59.5,
            duration_secs: 60.0,
        },
        &ep_id,
    );
    apply_writeback(
        &mut store,
        &AudioReport::Paused {
            url: "u".into(),
            position_secs: 60.0,
        },
        &ep_id,
    );
    apply_writeback(
        &mut store,
        &AudioReport::ItemEnd { url: "u".into() },
        &ep_id,
    );

    // `position_for` returns `None` for a zero position (the canonical
    // "start from the beginning" sentinel — see `position_for_returns_none_when_zero`).
    // Before the fix this was `Some(60.0)` (the duration), so replay landed at the end.
    assert_eq!(
        store.position_for(&ep_id),
        None,
        "position must rewind to the start on natural completion so replay starts over"
    );
}

#[test]
fn item_end_serde_round_trips() {
    let report = AudioReport::ItemEnd {
        url: "https://ex.com/ep.mp3".into(),
    };
    let json = serde_json::to_string(&report).expect("encode");
    assert!(json.contains("\"type\":\"item_end\""));
    let decoded: AudioReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, report);
}
// maybe_auto_advance: stage-and-dispatch atomicity (D6 null-app guard).
fn make_handle_for_advance(store: Arc<Mutex<crate::store::PodcastStore>>) -> Box<crate::ffi::handle::PodcastHandle> {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    let rev = Arc::new(AtomicU64::new(1));
    let identity = Arc::new(Mutex::new(crate::store::identity::IdentityStore::new()));
    let feedback = nmp_feedback::FeedbackRuntime::new(
        nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
            .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE),
        Arc::new(Mutex::new(Vec::new())),
        rev.clone(),
    );
    let state = Arc::new(crate::state::PodcastAppState::new_with_identity(
        crate::state::Infra::for_test_with_rev(rev.clone()),
        store.clone(),
        identity.clone(),
        feedback,
    ));
    Box::new(crate::ffi::handle::PodcastHandle {
        app: std::ptr::null_mut(),
        state,
        responder_cache: Arc::new(Mutex::new(crate::store::agent_note_responder_cache::ResponderCache::default())),
        outbound_turn_cache: Arc::new(Mutex::new(crate::store::outbound_turn_cache::OutboundTurnCache::new())),
        approved_peer_store: Arc::new(Mutex::new(crate::store::approved_peer_store::ApprovedPeerStore::new())),
        snapshot_cache: Arc::new(Mutex::new(None)),
        clean_html_cache: Arc::new(Mutex::new(HashMap::new())),
    })
}

fn actor_episode_id(handle: &crate::ffi::handle::PodcastHandle) -> Option<String> {
    handle.state.playback.player.lock().unwrap().state().episode_id.clone()
}

/// Happy-path: `maybe_auto_advance` must stage the next episode on the actor.
///
/// Before the fix, staging was in a separate guarded block from the dispatch —
/// on a poisoned lock, staging was skipped but dispatch still fired, leaving
/// `episode_id` as `None` while playback started (position never persists).
#[test]
fn auto_advance_stages_next_episode_on_actor() {
    let store = Arc::new(Mutex::new(crate::store::PodcastStore::new()));
    let podcast = podcast_core::Podcast::new("Auto-advance Show");
    let pid = podcast.id;
    let ep1 = make_episode(pid, "Episode 1");
    let ep2 = make_episode(pid, "Episode 2");
    let ep2_id = ep2.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![ep1, ep2]);
    let handle = make_handle_for_advance(store.clone());
    handle.state.playback.player.lock().unwrap().set_auto_play_next(true);
    handle.state.playback.queue.lock().unwrap().add_to_end(&ep2_id);
    maybe_auto_advance(&handle);
    assert_eq!(
        actor_episode_id(&handle),
        Some(ep2_id.to_lowercase()),
        "auto-advance must stage the next episode on the actor"
    );
}

/// Empty-queue: `maybe_auto_advance` must return early — no staging, no
/// dispatch.  With a null `app` pointer the pre-fix unconditional dispatch
/// path would crash (no D6 guard in the free functions).
#[test]
fn auto_advance_no_op_when_queue_is_empty() {
    let store = Arc::new(Mutex::new(crate::store::PodcastStore::new()));
    let podcast = podcast_core::Podcast::new("Single Show");
    let pid = podcast.id;
    store.lock().unwrap().subscribe(podcast, vec![make_episode(pid, "Only")]);
    let handle = make_handle_for_advance(store.clone());
    handle.state.playback.player.lock().unwrap().set_auto_play_next(true);
    maybe_auto_advance(&handle); // must not crash or stage anything
    assert_eq!(actor_episode_id(&handle), None);
}

/// `auto_play_next = false`: the flag gate must prevent staging and dispatch.
#[test]
fn auto_advance_no_op_when_auto_play_next_disabled() {
    let store = Arc::new(Mutex::new(crate::store::PodcastStore::new()));
    let podcast = podcast_core::Podcast::new("No-advance Show");
    let pid = podcast.id;
    let ep2 = make_episode(pid, "Ep 2");
    let ep2_id = ep2.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![make_episode(pid, "Ep 1"), ep2]);
    let handle = make_handle_for_advance(store.clone());
    handle.state.playback.player.lock().unwrap().set_auto_play_next(false);
    handle.state.playback.queue.lock().unwrap().add_to_end(&ep2_id);
    maybe_auto_advance(&handle);
    assert_eq!(actor_episode_id(&handle), None);
}
