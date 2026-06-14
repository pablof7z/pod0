//! Download auto-delete failure coverage for `audio_report` writeback.

use super::*;
use podcast_core::{Episode, Podcast};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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
            "nmp-audio-delete-{label}-{}-{n}",
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

#[test]
fn item_end_auto_delete_failure_keeps_download_state() {
    let dir = TempDir::new("failure");
    let not_a_file = dir.path.join("directory-path");
    std::fs::create_dir_all(&not_a_file).expect("create directory");

    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Auto Delete Failure");
    let podcast_id = podcast.id;
    let episode = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        "Episode",
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    );
    let typed_id = episode.id;
    let episode_id = episode.id.0.to_string();
    store.subscribe(podcast, vec![episode]);
    store.set_local_path(typed_id, not_a_file.to_string_lossy().into_owned(), 11);
    store.set_auto_mark_played_at_end(true);
    store.set_auto_delete_downloads_after_played(true);

    apply_writeback(
        &mut store,
        &AudioReport::ItemEnd { url: "u".into() },
        &episode_id,
    );

    assert!(
        store.local_path_for(&typed_id).is_some(),
        "failed auto-delete must keep the downloaded projection"
    );
    assert!(
        store
            .episode_events(&episode_id)
            .iter()
            .any(|event| event.kind == crate::store::events::stage::DOWNLOAD_DELETE_FAILED),
        "delete failure should be visible in diagnostics"
    );
}
