//! Tests for [`super::persistence`] — round-trip, atomic write, and backward-compat coverage.
//!
//! Extracted from `persistence.rs` to keep that file under the 500-line hard limit.

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
        let path =
            std::env::temp_dir().join(format!("nmp-podcast-persist-{}-{}", std::process::id(), n,));
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
            is_subscribed: true,
            auto_download: false,
            auto_download_mode: None,
            cellular_allowed: false,
            notifications_disabled: false,
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
        ..PersistedStore::default()
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
    let dir = TempDir::new();
    let raw = serde_json::json!({
        "schema_version": PERSIST_SCHEMA_VERSION,
        "podcasts": []
    });
    std::fs::write(podcasts_path(&dir.path), serde_json::to_vec(&raw).unwrap()).unwrap();
    let loaded = load(&dir.path).unwrap().expect("file present");
    assert!(loaded.memory_facts.is_empty());
}

#[test]
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
            ..PersistedSettings::default()
        },
        ..PersistedStore::default()
    };
    save(&dir.path, &payload).unwrap();
    let loaded = load(&dir.path).unwrap().expect("file present");
    assert_eq!(loaded.ad_segments.len(), 1);
    assert_eq!(loaded.ad_segments[0].0, "ep-1");
    assert_eq!(loaded.ad_segments[0].1[0].start_secs, 30.0);
    assert!(loaded.settings.auto_skip_ads_enabled);
}

#[test]
fn legacy_nostr_public_relays_setting_loads_and_drops_on_save() {
    let dir = TempDir::new();
    let raw = serde_json::json!({
        "schema_version": PERSIST_SCHEMA_VERSION,
        "podcasts": [],
        "settings": {
            "nostr_public_relays": ["wss://legacy.example"]
        }
    });
    std::fs::write(podcasts_path(&dir.path), serde_json::to_vec(&raw).unwrap()).unwrap();

    let loaded = load(&dir.path).unwrap().expect("file present");
    save(&dir.path, &loaded).unwrap();

    let saved = std::fs::read_to_string(podcasts_path(&dir.path)).unwrap();
    assert!(
        !saved.contains("nostr_public_relays"),
        "legacy relay mirror must not be persisted again: {saved}"
    );
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
        ..PersistedStore::default()
    };
    save(&dir.path, &payload).unwrap();
    let loaded = load(&dir.path).unwrap().expect("file present");
    assert_eq!(loaded.memory_facts.len(), 1);
    assert_eq!(loaded.memory_facts[0].key, "preferred_genre");
    assert_eq!(loaded.memory_facts[0].value, "technology");
    assert!(loaded.ad_segments.is_empty());
    // Canonical fresh-install default is ON (PodcastStore::new() →
    // PersistedSettings::default()).
    assert!(loaded.settings.auto_skip_ads_enabled);
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

#[test]
fn skip_intervals_persist_and_reload() {
    let dir = TempDir::new();
    let persisted = PersistedStore {
        schema_version: PERSIST_SCHEMA_VERSION,
        podcasts: vec![],
        settings: PersistedSettings {
            skip_forward_secs: 45.0,
            skip_backward_secs: 10.0,
            ..PersistedSettings::default()
        },
        ..PersistedStore::default()
    };
    save(&dir.path, &persisted).unwrap();
    let loaded = load(&dir.path).unwrap().expect("file present");
    assert!((loaded.settings.skip_forward_secs - 45.0).abs() < f64::EPSILON);
    assert!((loaded.settings.skip_backward_secs - 10.0).abs() < f64::EPSILON);
}
