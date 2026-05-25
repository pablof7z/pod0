//! Disk persistence for [`PodcastStore`].
//!
//! Single JSON file (`podcasts.json`) inside a caller-supplied data directory.
//! Writes are atomic (write to `podcasts.json.tmp` then rename); failures
//! degrade silently per D6 — the in-memory store stays authoritative.
//!
//! ## Wire format
//!
//! ```text
//! {
//!   "schema_version": 1,
//!   "podcasts": [ { "podcast": <Podcast>, "episodes": [<Episode>, ...] }, ... ]
//! }
//! ```
//!
//! Versioned so future migrations can detect older payloads. Unknown
//! schema_version is treated as "empty" — the file is replaced on next write.

use std::path::{Path, PathBuf};

use podcast_core::{Episode, Podcast};
use serde::{Deserialize, Serialize};

/// Schema marker for `podcasts.json`. Bump on incompatible format changes.
pub const PERSIST_SCHEMA_VERSION: u32 = 1;

/// File name of the persisted store inside the data directory.
pub const PODCASTS_FILE: &str = "podcasts.json";

/// On-disk envelope. One row per subscribed podcast with its episodes inlined
/// so the load is a single fread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedStore {
    pub schema_version: u32,
    pub podcasts: Vec<PersistedPodcast>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedPodcast {
    pub podcast: Podcast,
    #[serde(default)]
    pub episodes: Vec<Episode>,
}

/// Resolve the path of `podcasts.json` inside `data_dir`.
pub(super) fn podcasts_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PODCASTS_FILE)
}

/// Load `podcasts.json` from `data_dir`. Returns `Ok(None)` when the file
/// does not exist (fresh install). Any parse / IO error is propagated so the
/// caller can decide whether to log and continue with an empty store.
pub(super) fn load(data_dir: &Path) -> std::io::Result<Option<PersistedStore>> {
    let path = podcasts_path(data_dir);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let store: PersistedStore = serde_json::from_slice(&bytes).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?;
            if store.schema_version != PERSIST_SCHEMA_VERSION {
                // Unknown / future schema — treat as empty; the next mutation
                // will overwrite with the current shape.
                return Ok(None);
            }
            Ok(Some(store))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Atomically write `payload` to `podcasts.json` inside `data_dir`.
///
/// Strategy: serialize → write to `podcasts.json.tmp` → `fs::rename` over the
/// final path. `rename` is atomic on the same filesystem, so the only failure
/// modes are "old file intact" or "new file in place" — never a partial write.
pub(super) fn save(data_dir: &Path, payload: &PersistedStore) -> std::io::Result<()> {
    // Ensure the directory exists. `create_dir_all` is a no-op when present.
    std::fs::create_dir_all(data_dir)?;

    let json = serde_json::to_vec_pretty(payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let final_path = podcasts_path(data_dir);
    let tmp_path = data_dir.join(format!("{PODCASTS_FILE}.tmp"));
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, Podcast, PodcastId};
    use std::sync::atomic::{AtomicU64, Ordering};
    use uuid::Uuid;

    /// RAII tempdir that wipes itself on drop. Avoids pulling in the
    /// `tempfile` crate just for tests — keeps the dep graph tight.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nmp-podcast-persist-{}-{}",
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

    fn make_podcast(title: &str) -> Podcast {
        Podcast::new(title)
    }

    fn make_episode(podcast_id: PodcastId, title: &str) -> Episode {
        // Random guid so two `make_episode` calls produce distinct episode
        // ids (the store dedupes by id). With `Episode::new` now deriving the
        // id from `(feed_url, guid)`, randomness lives in the guid.
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
    fn load_returns_none_when_file_missing() {
        let dir = TempDir::new();
        assert!(load(&dir.path).unwrap().is_none());
    }

    #[test]
    fn save_then_load_round_trips_empty_store() {
        let dir = TempDir::new();
        let payload = PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: vec![],
        };
        save(&dir.path, &payload).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert_eq!(loaded.schema_version, PERSIST_SCHEMA_VERSION);
        assert_eq!(loaded.podcasts.len(), 0);
    }

    #[test]
    fn save_then_load_round_trips_podcasts_and_episodes() {
        let dir = TempDir::new();
        let podcast = make_podcast("Round Trip");
        let id = podcast.id;
        let episodes = vec![make_episode(id, "Ep 1"), make_episode(id, "Ep 2")];
        let payload = PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: vec![PersistedPodcast {
                podcast: podcast.clone(),
                episodes: episodes.clone(),
            }],
        };
        save(&dir.path, &payload).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert_eq!(loaded.podcasts.len(), 1);
        assert_eq!(loaded.podcasts[0].podcast, podcast);
        assert_eq!(loaded.podcasts[0].episodes, episodes);
    }

    #[test]
    fn save_creates_directory_if_missing() {
        let dir = TempDir::new();
        // Use a subdir that does not exist yet.
        let nested = dir.path.join("nested").join("library");
        let payload = PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: vec![],
        };
        save(&nested, &payload).unwrap();
        assert!(nested.join(PODCASTS_FILE).exists());
    }

    #[test]
    fn save_is_atomic_no_tmp_file_left_behind() {
        let dir = TempDir::new();
        let payload = PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: vec![],
        };
        save(&dir.path, &payload).unwrap();
        // After a successful save the .tmp file must be gone (renamed).
        assert!(!dir.path.join(format!("{PODCASTS_FILE}.tmp")).exists());
        assert!(dir.path.join(PODCASTS_FILE).exists());
    }

    #[test]
    fn unknown_schema_version_loads_as_none() {
        let dir = TempDir::new();
        // Write a payload with a future schema_version directly.
        let raw = serde_json::json!({
            "schema_version": 9999,
            "podcasts": []
        });
        std::fs::write(podcasts_path(&dir.path), serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(load(&dir.path).unwrap().is_none());
    }

    #[test]
    fn corrupted_file_is_an_error() {
        let dir = TempDir::new();
        std::fs::write(podcasts_path(&dir.path), b"not valid json").unwrap();
        assert!(load(&dir.path).is_err());
    }
}
