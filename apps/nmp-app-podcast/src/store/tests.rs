//! Integration tests for [`super::PodcastStore`].
//!
//! Lives in a sibling file so the implementation in `store/mod.rs` stays
//! under the AGENTS.md 500-line hard limit. Persistence-layer tests
//! remain colocated with their implementation in `store/persistence.rs`.
//!
//! Overflow tests (agent-memory, persistence integration, settings) live in
//! `store/tests_ext.rs`; shared helpers are re-exported `pub(super)` so that
//! sibling can import them without duplication.
use super::*;
use podcast_core::{Episode, Podcast, PodcastId};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// RAII tempdir for store integration tests. Same pattern as
/// `persistence::tests::TempDir`; pub(super) so `tests_ext` can share it.
pub(super) struct TempDir {
    pub(super) path: PathBuf,
}

impl TempDir {
    pub(super) fn new() -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nmp-podcast-store-{}-{}",
            std::process::id(),
            n,
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

pub(super) fn make_podcast(title: &str) -> Podcast {
    Podcast::new(title)
}

pub(super) fn make_episode(podcast_id: PodcastId, title: &str) -> Episode {
    Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        title,
        url::Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    )
}

#[test]
fn subscribe_and_retrieve() {
    let mut store = PodcastStore::new();
    let podcast = make_podcast("Test Show");
    let id = podcast.id;
    store.subscribe(podcast, vec![]);
    assert_eq!(store.podcast_count(), 1);
    assert!(store.podcast(id).is_some());
}

#[test]
fn all_podcasts_returns_all() {
    let mut store = PodcastStore::new();
    store.subscribe(make_podcast("Show A"), vec![]);
    store.subscribe(make_podcast("Show B"), vec![]);
    assert_eq!(store.all_podcasts().len(), 2);
}

#[test]
fn resubscribe_replaces_existing() {
    let mut store = PodcastStore::new();
    let p1 = make_podcast("Original Title");
    let id = p1.id;
    store.subscribe(p1, vec![]);

    let mut p2 = make_podcast("Updated Title");
    p2.id = id; // same id — should replace
    store.subscribe(p2, vec![]);
    assert_eq!(store.podcast_count(), 1);
    assert_eq!(store.podcast(id).map(|p| p.title.as_str()), Some("Updated Title"));
}

#[test]
fn set_and_get_local_path() {
    let mut store = PodcastStore::new();
    let ep_id = EpisodeId::generate();
    assert!(store.local_path_for(&ep_id).is_none());
    store.set_local_path(ep_id, "/tmp/ep.mp3".into());
    assert_eq!(store.local_path_for(&ep_id), Some("/tmp/ep.mp3"));
}

#[test]
fn clear_local_path_returns_previous_and_unsets() {
    let mut store = PodcastStore::new();
    let ep_id = EpisodeId::generate();
    store.set_local_path(ep_id, "/tmp/ep.mp3".into());
    let prev = store.clear_local_path(&ep_id);
    assert_eq!(prev.as_deref(), Some("/tmp/ep.mp3"));
    assert!(store.local_path_for(&ep_id).is_none());
    assert!(store.clear_local_path(&ep_id).is_none());
}

// ── Playback position writeback ──────────────────────────────────────

#[test]
fn set_episode_position_updates_in_memory() {
    let mut store = PodcastStore::new();
    let podcast = make_podcast("Resume Show");
    let id = podcast.id;
    let ep = make_episode(id, "Ep 1");
    let ep_id_str = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    assert_eq!(store.position_for(&ep_id_str), None);
    let updated = store.set_episode_position(&ep_id_str, 42.5);
    assert!(updated);
    assert_eq!(store.position_for(&ep_id_str), Some(42.5));
}

#[test]
fn set_episode_position_returns_false_for_unknown_id() {
    let mut store = PodcastStore::new();
    // Subscribe with no episodes so we exercise the negative path against
    // a real (non-empty) store.
    store.subscribe(make_podcast("Empty"), vec![]);
    assert!(!store.set_episode_position("00000000-0000-0000-0000-000000000000", 1.0));
}

#[test]
fn position_for_returns_none_when_zero() {
    // The projection treats `0.0` as "no resume point" so the UI doesn't
    // render "Resume at 0:00" on fresh episodes.
    let mut store = PodcastStore::new();
    let podcast = make_podcast("Zero Show");
    let id = podcast.id;
    let ep = make_episode(id, "Ep 1");
    let ep_id_str = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);
    assert_eq!(store.position_for(&ep_id_str), None);
}

#[test]
fn set_episode_position_clamps_negative_to_zero() {
    let mut store = PodcastStore::new();
    let podcast = make_podcast("Clamp Show");
    let id = podcast.id;
    let ep = make_episode(id, "Ep 1");
    let ep_id_str = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);
    store.set_episode_position(&ep_id_str, -5.0);
    // Clamped to 0 ⇒ surfaces as None per the "no resume at 0" rule.
    assert_eq!(store.position_for(&ep_id_str), None);
}

#[test]
fn set_episode_position_does_not_persist_until_flush() {
    // The whole point of the in-memory path: do not burn disk on every
    // ≤4 Hz `Playing` tick. Position mutations must not touch disk;
    // `flush_positions` is the explicit checkpoint.
    let dir = TempDir::new();
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = make_podcast("Throttle Show");
    let pid = podcast.id;
    let ep = make_episode(pid, "Ep 1");
    let ep_id_str = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);

    // Bump position several times — disk file should still hold pos=0.
    store.set_episode_position(&ep_id_str, 12.0);
    store.set_episode_position(&ep_id_str, 24.0);
    store.set_episode_position(&ep_id_str, 36.0);

    let store2 = {
        let mut s = PodcastStore::new();
        s.set_data_dir(dir.path.clone());
        s
    };
    assert_eq!(store2.position_for(&ep_id_str), None);

    // Flush the writes and reload — position must round-trip.
    store.flush_positions();
    let mut store3 = PodcastStore::new();
    store3.set_data_dir(dir.path.clone());
    assert_eq!(store3.position_for(&ep_id_str), Some(36.0));
}

// ── Auto-download flag ──────────────────────────────────────────────────

#[test]
fn auto_download_defaults_to_false() {
    let store = PodcastStore::new();
    let id = PodcastId::generate();
    assert!(!store.is_auto_download_enabled(id));
}

#[test]
fn set_auto_download_toggles_flag() {
    let mut store = PodcastStore::new();
    let id = PodcastId::generate();
    store.set_auto_download(id, true);
    assert!(store.is_auto_download_enabled(id));
    store.set_auto_download(id, false);
    assert!(!store.is_auto_download_enabled(id));
}

#[test]
fn set_auto_download_is_idempotent() {
    let mut store = PodcastStore::new();
    let id = PodcastId::generate();
    store.set_auto_download(id, true);
    store.set_auto_download(id, true);
    assert!(store.is_auto_download_enabled(id));
}

#[test]
fn unsubscribe_clears_auto_download_flag() {
    let mut store = PodcastStore::new();
    let podcast = make_podcast("Toggled");
    let id = podcast.id;
    store.subscribe(podcast, vec![]);
    store.set_auto_download(id, true);
    assert!(store.is_auto_download_enabled(id));

    store.unsubscribe(id);
    assert!(!store.is_auto_download_enabled(id));
}

#[test]
fn is_auto_download_enabled_str_handles_invalid_uuid() {
    let store = PodcastStore::new();
    assert!(!store.is_auto_download_enabled_str("not-a-uuid"));
}

#[test]
fn is_auto_download_enabled_str_matches_set_state() {
    let mut store = PodcastStore::new();
    let podcast = make_podcast("Show");
    let id = podcast.id;
    store.subscribe(podcast, vec![]);
    store.set_auto_download(id, true);
    assert!(store.is_auto_download_enabled_str(&id.0.to_string()));
}

#[test]
fn auto_download_flag_persists_across_reload() {
    let dir = TempDir::new();
    let podcast_id;
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = make_podcast("Persistent Auto");
        podcast_id = podcast.id;
        store.subscribe(podcast, vec![]);
        store.set_auto_download(podcast_id, true);
    }
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert!(store2.is_auto_download_enabled(podcast_id));
}
