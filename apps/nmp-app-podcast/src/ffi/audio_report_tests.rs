//! Tests for [`super::audio_report`] — playback-position writeback and flush-throttle coverage.
//!
//! Extracted from `audio_report.rs` to keep that file under the 500-line hard limit.

use super::*;
use podcast_core::{Episode, Podcast, PodcastId};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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
        &AudioReport::Playing { url: "u".into(), position_secs: 59.5, duration_secs: 60.0 },
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
    assert!(played_in_memory, "episode must be marked played after ItemEnd");

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
fn item_end_serde_round_trips() {
    let report = AudioReport::ItemEnd { url: "https://ex.com/ep.mp3".into() };
    let json = serde_json::to_string(&report).expect("encode");
    assert!(json.contains("\"type\":\"item_end\""));
    let decoded: AudioReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, report);
}
