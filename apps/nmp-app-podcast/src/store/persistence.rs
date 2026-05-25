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
//!   "podcasts": [ { "podcast": <Podcast>, "episodes": [<Episode>, ...] }, ... ],
//!   "memory_facts": [ { "id": "...", "key": "...", ... }, ... ]  // optional
//! }
//! ```
//!
//! Versioned so future migrations can detect older payloads. Unknown
//! schema_version is treated as "empty" — the file is replaced on next
//! write. New optional fields (e.g. `memory_facts` added in feature #33)
//! are tagged `#[serde(default)]` so older payloads decode cleanly without
//! bumping the schema and wiping every subscription on upgrade.

use std::path::{Path, PathBuf};

use podcast_core::{Episode, Podcast};
use serde::{Deserialize, Serialize};

use crate::ffi::projections::MemoryFact;
use crate::player::AdSegment;

/// Schema marker for `podcasts.json`. Bump on incompatible format changes.
pub const PERSIST_SCHEMA_VERSION: u32 = 1;

/// File name of the persisted store inside the data directory.
pub const PODCASTS_FILE: &str = "podcasts.json";

/// On-disk envelope. One row per subscribed podcast with its episodes inlined
/// so the load is a single fread.
///
/// `has_completed_onboarding` is part of the same envelope so the iOS
/// shell's `OnboardingView` gate survives restart without a second file.
/// `serde(default)` keeps older saved files (predating the field) loading
/// cleanly as `false`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedStore {
    pub schema_version: u32,
    pub podcasts: Vec<PersistedPodcast>,
    #[serde(default)]
    pub has_completed_onboarding: bool,
    /// Agent memory bag. Optional on the wire so existing v1 payloads
    /// (written before feature #33) decode without losing podcasts.
    #[serde(default)]
    pub memory_facts: Vec<MemoryFact>,
/// `ad_segments` and `settings` are `#[serde(default)]` so payloads written
/// before this PR landed (no ad-skip support) still load cleanly into a
/// store with auto-skip-ads off and no per-episode segments.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct PersistedStore {
    pub schema_version: u32,
    pub podcasts: Vec<PersistedPodcast>,
    /// `episode_id` (UUID string) → ad-break intervals. Sorted on
    /// write for deterministic on-disk bytes.
    #[serde(default)]
    pub ad_segments: Vec<(String, Vec<AdSegment>)>,
    #[serde(default)]
    pub settings: PersistedSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct PersistedSettings {
    /// Mirrors `PodcastStore::auto_skip_ads_enabled`. Defaults to
    /// `false` so an old payload (no settings block) hydrates with
    /// the toggle off — never accidentally enabled.
    #[serde(default)]
    pub auto_skip_ads_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedPodcast {
    pub podcast: Podcast,
    #[serde(default)]
    pub episodes: Vec<Episode>,
    /// Per-podcast auto-download opt-in flag. `#[serde(default)]` lets the
    /// load path tolerate older `podcasts.json` files written before this
    /// field shipped: missing key ⇒ `false` (auto-download off). We
    /// deliberately do NOT bump `PERSIST_SCHEMA_VERSION` for this addition
    /// — bumping wipes the user's library because `load()` treats unknown
    /// schemas as empty (see this file, line ~60).
    #[serde(default)]
    pub auto_download: bool,
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
            has_completed_onboarding: false,
            memory_facts: vec![],
            ..PersistedStore::default()
        };
        save(&dir.path, &payload).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert_eq!(loaded.schema_version, PERSIST_SCHEMA_VERSION);
        assert_eq!(loaded.podcasts.len(), 0);
        assert!(!loaded.has_completed_onboarding);
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
                auto_download: false,
            }],
            has_completed_onboarding: false,
            memory_facts: vec![],
            ..PersistedStore::default()
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
            has_completed_onboarding: false,
            memory_facts: vec![],
            ..PersistedStore::default()
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
            has_completed_onboarding: false,
            memory_facts: vec![],
            ..PersistedStore::default()
        };
        save(&dir.path, &payload).unwrap();
        // After a successful save the .tmp file must be gone (renamed).
        assert!(!dir.path.join(format!("{PODCASTS_FILE}.tmp")).exists());
        assert!(dir.path.join(PODCASTS_FILE).exists());
    }

    #[test]
    fn save_then_load_round_trips_has_completed_onboarding() {
        let dir = TempDir::new();
        let payload = PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: vec![],
            has_completed_onboarding: true,
        };
        save(&dir.path, &payload).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert!(loaded.has_completed_onboarding);
    }

    #[test]
    fn legacy_envelope_without_onboarding_field_loads_as_false() {
        // Forward compat: an older `podcasts.json` predating the settings
        // projection lacks the `has_completed_onboarding` field. `serde(default)`
        // must keep the load succeeding and produce `false` for the flag.
        let dir = TempDir::new();
        let raw = serde_json::json!({
            "schema_version": PERSIST_SCHEMA_VERSION,
            "podcasts": []
        });
        std::fs::write(podcasts_path(&dir.path), serde_json::to_vec(&raw).unwrap()).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert!(!loaded.has_completed_onboarding);
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
    fn legacy_payload_without_memory_facts_loads_with_empty_default() {
        // A v1 file written before feature #33 has no `memory_facts` field;
        // it must still load (with an empty bag) so users don't lose their
        // subscriptions on upgrade.
    fn round_trip_preserves_ad_segments_and_settings() {
        use podcast_core::AdKind;
        let dir = TempDir::new();
        let payload = PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: vec![],
            ad_segments: vec![(
                "ep-1".into(),
                vec![AdSegment::new(30.0, 60.0, AdKind::Midroll)],
            )],
            settings: PersistedSettings {
                auto_skip_ads_enabled: true,
            },
        };
        save(&dir.path, &payload).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert_eq!(loaded.ad_segments.len(), 1);
        assert_eq!(loaded.ad_segments[0].0, "ep-1");
        assert_eq!(loaded.ad_segments[0].1[0].start_secs, 30.0);
        assert!(loaded.settings.auto_skip_ads_enabled);
    }

    #[test]
    fn pre_ad_skip_payload_loads_with_defaults() {
        // An on-disk file written before this PR landed has no
        // `ad_segments` or `settings` fields. The decoder must hydrate
        // them as empty/false so the user doesn't get the toggle
        // surprise-enabled.
        let dir = TempDir::new();
        let raw = serde_json::json!({
            "schema_version": PERSIST_SCHEMA_VERSION,
            "podcasts": []
        });
        std::fs::write(podcasts_path(&dir.path), serde_json::to_vec(&raw).unwrap()).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert!(loaded.memory_facts.is_empty());
        assert_eq!(loaded.podcasts.len(), 0);
    }

    #[test]
    fn save_then_load_round_trips_memory_facts() {
        let dir = TempDir::new();
        let payload = PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: vec![],
            memory_facts: vec![MemoryFact {
                id: "preferred_genre".into(),
                key: "preferred_genre".into(),
                value: "technology".into(),
                source: "user".into(),
                created_at: 1_700_000_000,
            }],
        };
        save(&dir.path, &payload).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert_eq!(loaded.memory_facts.len(), 1);
        assert_eq!(loaded.memory_facts[0].key, "preferred_genre");
        assert_eq!(loaded.memory_facts[0].value, "technology");
        assert!(loaded.ad_segments.is_empty());
        assert!(!loaded.settings.auto_skip_ads_enabled);
    }

    #[test]
    fn corrupted_file_is_an_error() {
        let dir = TempDir::new();
        std::fs::write(podcasts_path(&dir.path), b"not valid json").unwrap();
        assert!(load(&dir.path).is_err());
    }

    #[test]
    fn load_tolerates_missing_auto_download_field() {
        // Backward-compat: a `podcasts.json` written before the auto_download
        // field shipped must load with auto_download = false (the field
        // default) — never panic and never bump schema_version (which would
        // wipe the library, see load() schema_version branch).
        let dir = TempDir::new();
        let podcast = make_podcast("Legacy Show");
        // Build the payload WITHOUT the `auto_download` key — mirrors an
        // older app version's on-disk format.
        let raw = serde_json::json!({
            "schema_version": PERSIST_SCHEMA_VERSION,
            "podcasts": [{
                "podcast": podcast,
                "episodes": []
            }]
        });
        std::fs::write(podcasts_path(&dir.path), serde_json::to_vec(&raw).unwrap()).unwrap();
        let loaded = load(&dir.path).unwrap().expect("file present");
        assert_eq!(loaded.podcasts.len(), 1);
        assert!(!loaded.podcasts[0].auto_download);
    }
}
