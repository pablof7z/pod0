//! Tests for download lifecycle host-op handlers.

use super::*;
use crate::state::{Infra, PodcastAppState};
use crate::store::PodcastStore;
use podcast_core::{Episode, EpisodeId, Podcast};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nmp-download-delete-{label}-{}-{n}",
            std::process::id()
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

fn handler_with_store(store: Arc<Mutex<PodcastStore>>) -> PodcastHostOpHandler {
    let state = Arc::new(PodcastAppState::new(Infra::for_test(), store));
    PodcastHostOpHandler::new(std::ptr::null_mut(), state)
}

fn seed_download(store: &mut PodcastStore, local_path: String) -> (String, EpisodeId) {
    let podcast = Podcast::new("Delete Show");
    let podcast_id = podcast.id;
    let episode = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        "Episode",
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    );
    let episode_id = episode.id;
    let episode_id_str = episode.id.0.to_string();
    store.subscribe(podcast, vec![episode]);
    store.set_local_path(episode_id, local_path, 11);
    (episode_id_str, episode_id)
}

#[test]
fn delete_download_removes_file_then_clears_projection() {
    let dir = TempDir::new("success");
    let file = dir.path.join("episode.mp3");
    std::fs::write(&file, b"audio bytes").expect("write fixture file");
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (episode_id, typed_id) = {
        let mut s = store.lock().unwrap();
        seed_download(&mut s, file.to_string_lossy().into_owned())
    };
    let handler = handler_with_store(Arc::clone(&store));

    let result = handler.handle_delete_download(episode_id.clone());

    assert_eq!(result["ok"], serde_json::json!(true));
    assert!(!file.exists(), "file should be removed from disk");
    let mut s = store.lock().unwrap();
    assert!(s.local_path_for(&typed_id).is_none());
    assert!(s
        .episode_events(&episode_id)
        .iter()
        .any(|event| event.kind == crate::store::events::stage::DOWNLOAD_DELETED));
}

#[test]
fn delete_download_clears_stale_missing_file_path() {
    let dir = TempDir::new("missing");
    let missing = dir.path.join("already-gone.mp3");
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (episode_id, typed_id) = {
        let mut s = store.lock().unwrap();
        seed_download(&mut s, missing.to_string_lossy().into_owned())
    };
    let handler = handler_with_store(Arc::clone(&store));

    let result = handler.handle_delete_download(episode_id);

    assert_eq!(result["ok"], serde_json::json!(true));
    assert!(store.lock().unwrap().local_path_for(&typed_id).is_none());
}

#[test]
fn delete_download_failure_keeps_projection_downloaded() {
    let dir = TempDir::new("failure");
    let not_a_file = dir.path.join("directory-path");
    std::fs::create_dir_all(&not_a_file).expect("create directory");
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (episode_id, typed_id) = {
        let mut s = store.lock().unwrap();
        seed_download(&mut s, not_a_file.to_string_lossy().into_owned())
    };
    let handler = handler_with_store(Arc::clone(&store));

    let result = handler.handle_delete_download(episode_id.clone());

    assert_eq!(result["ok"], serde_json::json!(false));
    assert!(
        store.lock().unwrap().local_path_for(&typed_id).is_some(),
        "failed deletion must not clear the downloaded projection"
    );
    let mut s = store.lock().unwrap();
    let failure = s
        .episode_events(&episode_id)
        .into_iter()
        .find(|event| event.kind == crate::store::events::stage::DOWNLOAD_DELETE_FAILED)
        .expect("delete failure event");
    assert_eq!(failure.severity, "failure");
}
