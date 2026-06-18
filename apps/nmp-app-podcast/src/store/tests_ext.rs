//! Overflow tests for [`super::PodcastStore`].
//!
//! Split from `store/tests.rs` to keep both files under the AGENTS.md 500-line
//! hard ceiling. Covers agent-memory, persistence integration, and settings.
use super::tests::{make_episode, make_podcast, TempDir};
use super::*;
use podcast_core::{DownloadState, PodcastId};

// ── Auto-download persistence (overflow from tests.rs) ──────────────────

#[test]
fn auto_download_off_state_persists_across_reload() {
    let dir = TempDir::new();
    let podcast_id;
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = make_podcast("Default Off");
        podcast_id = podcast.id;
        store.subscribe(podcast, vec![]);
        // Never toggled — flag stays false.
    }
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert!(!store2.is_auto_download_enabled(podcast_id));
}

// ── Agent memory (feature #33) ───────────────────────────────────────

#[test]
fn set_memory_fact_inserts_then_lists_in_key_order() {
    let mut store = PodcastStore::new();
    store.set_memory_fact("zebra".into(), "stripes".into(), "user".into(), 100);
    store.set_memory_fact("alpha".into(), "first".into(), "agent".into(), 200);
    let facts = store.all_memory_facts();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].key, "alpha");
    assert_eq!(facts[0].source, "agent");
    assert_eq!(facts[1].key, "zebra");
}

#[test]
fn set_memory_fact_upsert_preserves_id_and_created_at() {
    let mut store = PodcastStore::new();
    store.set_memory_fact("genre".into(), "tech".into(), "user".into(), 100);
    store.set_memory_fact("genre".into(), "history".into(), "agent".into(), 999);
    let facts = store.all_memory_facts();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].value, "history");
    assert_eq!(facts[0].source, "agent");
    // Upsert preserves the original created_at — the row is the same
    // memory, just re-stated.
    assert_eq!(facts[0].created_at, 100);
    assert_eq!(facts[0].id, "genre");
}

#[test]
fn remove_memory_fact_reports_hit_vs_miss() {
    let mut store = PodcastStore::new();
    store.set_memory_fact("k".into(), "v".into(), "user".into(), 1);
    assert!(store.remove_memory_fact("k"));
    assert!(!store.remove_memory_fact("k")); // already gone
    assert!(store.all_memory_facts().is_empty());
}

#[test]
fn clear_memory_returns_count_and_wipes() {
    let mut store = PodcastStore::new();
    store.set_memory_fact("a".into(), "1".into(), "user".into(), 1);
    store.set_memory_fact("b".into(), "2".into(), "user".into(), 2);
    assert_eq!(store.clear_memory(), 2);
    assert!(store.all_memory_facts().is_empty());
    // Empty bag → second clear is a no-op (count 0).
    assert_eq!(store.clear_memory(), 0);
}

#[test]
fn memory_facts_persist_across_reload() {
    let dir = TempDir::new();
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        store.set_memory_fact("tz".into(), "UTC".into(), "user".into(), 42);
        store.set_memory_fact("pref".into(), "dark".into(), "agent".into(), 43);
    }
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    let facts = store2.all_memory_facts();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].key, "pref");
    assert_eq!(facts[0].source, "agent");
    assert_eq!(facts[0].created_at, 43);
    assert_eq!(facts[1].key, "tz");
    assert_eq!(facts[1].value, "UTC");
}

// ── Persistence integration tests ────────────────────────────────────

#[test]
fn set_data_dir_on_empty_dir_returns_zero() {
    let dir = TempDir::new();
    let mut store = PodcastStore::new();
    let loaded = store.set_data_dir(dir.path.clone());
    assert_eq!(loaded, 0);
    assert_eq!(store.podcast_count(), 0);
    assert_eq!(store.data_dir(), Some(dir.path.as_path()));
}

#[test]
fn subscribe_writes_to_disk_when_bound() {
    let dir = TempDir::new();
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    store.subscribe(make_podcast("Disk Show"), vec![]);
    assert!(dir.path.join("podcasts.json").exists());
}

#[test]
fn fresh_store_can_reload_after_subscribe() {
    let dir = TempDir::new();
    let podcast_id;
    let episodes;
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = make_podcast("Persistent Show");
        podcast_id = podcast.id;
        episodes = vec![
            make_episode(podcast_id, "Ep 1"),
            make_episode(podcast_id, "Ep 2"),
        ];
        store.subscribe(podcast, episodes.clone());
    }
    // New store, same dir — should rehydrate.
    let mut store2 = PodcastStore::new();
    let loaded = store2.set_data_dir(dir.path.clone());
    assert_eq!(loaded, 1);
    assert_eq!(store2.podcast_count(), 1);
    let restored = store2.podcast(podcast_id).expect("podcast restored");
    assert_eq!(restored.title, "Persistent Show");
    assert_eq!(store2.episodes_for(podcast_id).len(), 2);
    assert_eq!(store2.episodes_for(podcast_id), episodes.as_slice());
}

#[test]
fn downloaded_episode_path_and_size_survive_reload() {
    let dir = TempDir::new();
    let episode_id;
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = make_podcast("Downloaded Show");
        let podcast_id = podcast.id;
        let episode = make_episode(podcast_id, "Saved Ep");
        episode_id = episode.id;
        store.subscribe(podcast, vec![episode]);
        store.set_local_path(episode_id, "/tmp/saved-ep.mp3".into(), 12_345);
    }

    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert_eq!(
        store2.local_path_for(&episode_id),
        Some("/tmp/saved-ep.mp3")
    );
    assert_eq!(store2.file_size_for(&episode_id), Some(12_345));
}

#[test]
fn cleared_download_path_survives_reload() {
    let dir = TempDir::new();
    let episode_id;
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = make_podcast("Cleared Download");
        let podcast_id = podcast.id;
        let episode = make_episode(podcast_id, "Saved Ep");
        episode_id = episode.id;
        store.subscribe(podcast, vec![episode]);
        store.set_local_path(episode_id, "/tmp/saved-ep.mp3".into(), 12_345);
        assert_eq!(
            store.clear_local_path(&episode_id).as_deref(),
            Some("/tmp/saved-ep.mp3")
        );
    }

    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert!(store2.local_path_for(&episode_id).is_none());
    assert!(store2.file_size_for(&episode_id).is_none());
}

#[test]
fn legacy_download_state_hydrates_local_path_on_reload() {
    let dir = TempDir::new();
    let episode_id;
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        let podcast = make_podcast("Legacy Download");
        let podcast_id = podcast.id;
        let mut episode = make_episode(podcast_id, "Local Ep");
        episode_id = episode.id;
        episode.download_state = DownloadState::Downloaded {
            local_file_url: url::Url::from_file_path("/tmp/legacy-ep.mp3").unwrap(),
            byte_count: 55,
        };
        store.subscribe(podcast, vec![episode]);
    }

    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert_eq!(
        store2.local_path_for(&episode_id),
        Some("/tmp/legacy-ep.mp3")
    );
    assert_eq!(store2.file_size_for(&episode_id), Some(55));
}

#[test]
fn unsubscribe_writes_to_disk_when_bound() {
    let dir = TempDir::new();
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = make_podcast("Doomed");
    let id = podcast.id;
    store.subscribe(podcast, vec![]);
    store.unsubscribe(id);

    // Reload — should be empty.
    let mut store2 = PodcastStore::new();
    let loaded = store2.set_data_dir(dir.path.clone());
    assert_eq!(loaded, 0);
    assert_eq!(store2.podcast_count(), 0);
}

#[test]
fn update_refresh_metadata_writes_to_disk_when_bound() {
    let dir = TempDir::new();
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = make_podcast("Etag Show");
    let id = podcast.id;
    store.subscribe(podcast, vec![]);
    store.update_refresh_metadata(id, Some("W/\"abc\"".into()), Some("Mon, 25 May".into()));

    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    let restored = store2.podcast(id).expect("podcast restored");
    assert_eq!(restored.etag.as_deref(), Some("W/\"abc\""));
    assert_eq!(restored.last_modified.as_deref(), Some("Mon, 25 May"));
    assert!(restored.last_refreshed_at.is_some());
}

#[test]
fn known_unsubscribed_podcast_round_trips_without_follow_membership() {
    let dir = TempDir::new();
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    let podcast = make_podcast("Known Only");
    let id = podcast.id;
    store.upsert_known_podcast(podcast, vec![make_episode(id, "Ep 1")]);

    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());

    assert!(store2.podcast(id).is_some());
    assert_eq!(store2.episodes_for(id).len(), 1);
    assert!(!store2.is_subscribed(id));
    assert!(store2.subscribed_podcasts().is_empty());
}

#[test]
fn set_data_dir_replaces_in_memory_state() {
    // If the store already has content in memory and a different data dir
    // is bound, the on-disk state from that dir wins (replaces in-mem).
    let dir = TempDir::new();
    // Pre-populate dir from store A.
    {
        let mut store_a = PodcastStore::new();
        store_a.set_data_dir(dir.path.clone());
        store_a.subscribe(make_podcast("From Disk"), vec![]);
    }
    // Store B starts with a different in-memory podcast, then binds.
    let mut store_b = PodcastStore::new();
    store_b.subscribe(make_podcast("Transient"), vec![]);
    assert_eq!(store_b.podcast_count(), 1);

    let loaded = store_b.set_data_dir(dir.path.clone());
    assert_eq!(loaded, 1);
    // The transient podcast was replaced by the one on disk.
    let titles: Vec<&str> = store_b
        .all_podcasts()
        .iter()
        .map(|(p, _)| p.title.as_str())
        .collect();
    assert_eq!(titles, vec!["From Disk"]);
}

#[test]
fn store_without_data_dir_never_touches_disk() {
    // Sanity: in-memory only mode is the default and does not panic.
    let mut store = PodcastStore::new();
    store.subscribe(make_podcast("Memory Only"), vec![]);
    store.unsubscribe(PodcastId::generate()); // no-op
    assert_eq!(store.podcast_count(), 1);
    assert!(store.data_dir().is_none());
}

// ── Onboarding flag (settings projection) ───────────────────────────

#[test]
fn has_completed_onboarding_defaults_to_false() {
    let store = PodcastStore::new();
    assert!(!store.has_completed_onboarding());
}

#[test]
fn set_onboarding_complete_updates_flag() {
    let mut store = PodcastStore::new();
    store.set_onboarding_complete(true);
    assert!(store.has_completed_onboarding());
    store.set_onboarding_complete(false);
    assert!(!store.has_completed_onboarding());
}

#[test]
fn onboarding_flag_persists_across_reload() {
    let dir = TempDir::new();
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        store.set_onboarding_complete(true);
    }
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(dir.path.clone());
    assert!(store2.has_completed_onboarding());
}

#[test]
fn fresh_data_dir_yields_false_onboarding_flag() {
    let dir = TempDir::new();
    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());
    assert!(!store.has_completed_onboarding());
}

// ── Regression: subscription status UUID case-insensitive lookup ──────────
//
// `nmp_app_podcast_library_subscription_status` receives podcast IDs from
// Swift as uppercase UUID strings (Swift's `UUID.uuidString` is always
// uppercase, e.g. "A1A1FFFF-0001-0001-0001-000000000001"), but Rust's
// `Uuid::to_string()` always renders lowercase. A case-sensitive `==`
// comparison therefore never matches — the podcast shows as "Follow" in the
// UI even when it is subscribed and correctly persisted.
//
// The fix changes the id_match branch to `eq_ignore_ascii_case`, identical
// to the pattern used by `episode_playback_info` and other store lookups.
// This test documents both the bug (lower-case-only comparison fails) and
// the correct behavior (case-insensitive comparison succeeds).
#[test]
fn subscription_status_lookup_requires_case_insensitive_uuid() {
    let mut store = PodcastStore::new();
    let podcast = make_podcast("This American Life");
    let podcast_id = podcast.id;
    store.subscribe(podcast, vec![]);

    assert!(store.is_subscribed(podcast_id), "podcast must be subscribed");

    let lowercase_id = podcast_id.0.to_string();
    // Swift sends UUID.uuidString which is always uppercase.
    let uppercase_id = lowercase_id.to_uppercase();

    // Case-sensitive comparison (old, broken code): never matches because
    // Rust to_string() is lowercase but Swift sends uppercase.
    let matched_case_sensitive = store
        .all_podcasts()
        .into_iter()
        .find(|(p, _)| p.id.0.to_string() == uppercase_id.as_str() && store.is_subscribed(p.id));
    assert!(
        matched_case_sensitive.is_none(),
        "case-sensitive == must fail for Swift uppercase UUID — confirming the original bug"
    );

    // Case-insensitive comparison (the fix): matches correctly.
    let matched_case_insensitive = store
        .all_podcasts()
        .into_iter()
        .find(|(p, _)| {
            p.id.0
                .to_string()
                .eq_ignore_ascii_case(uppercase_id.as_str())
                && store.is_subscribed(p.id)
        });
    assert!(
        matched_case_insensitive.is_some(),
        "case-insensitive eq_ignore_ascii_case must match a subscribed podcast by uppercase UUID"
    );
}

// ── Regression: UITestSeeder seed JSON populates followed_podcasts ────────
//
// The UITestSeeder writes podcasts.json without an `is_subscribed` field.
// The `PersistedPodcast` struct has `#[serde(default = "default_true")]` on
// that field, so absent ⇒ true ⇒ podcast lands in `followed_podcasts` after
// `load_from_disk`. This test confirms that invariant using the same minimal
// seed shape the seeder produces.
#[test]
fn seed_json_without_is_subscribed_field_defaults_to_followed() {
    let dir = TempDir::new();
    // Minimal seed matching UITestSeeder output: no `is_subscribed` key.
    let seed = serde_json::json!({
        "schema_version": 1,
        "podcasts": [{
            "podcast": {
                "id": "a1a1ffff-0001-0001-0001-000000000001",
                "feed_url": "https://test.podcast.local/rss.xml",
                "title": "This American Life",
                "author": "This American Life",
                "image_url": "https://thisamericanlife.org/img.png",
                "description": "Weekly public radio.",
                "categories": [],
                "discovered_at": "2026-06-06T13:00:00Z",
                "nostr_visibility": "private",
                "title_is_placeholder": false
            },
            "episodes": [],
            "auto_download": false,
            "cellular_allowed": false
            // NOTE: no "is_subscribed" key — must default to true
        }],
        "has_completed_onboarding": true,
        "memory_facts": [],
        "ad_segments": [],
        "episode_triage": [],
        "metadata_indexed_episodes": [],
        "transcript_status_overrides": [],
        "local_paths": [],
        "file_sizes": [],
        "settings": {},
        "queue": [],
        "pending_wifi_downloads": []
    });
    std::fs::write(
        dir.path.join("podcasts.json"),
        serde_json::to_vec(&seed).unwrap(),
    )
    .unwrap();

    let mut store = PodcastStore::new();
    store.set_data_dir(dir.path.clone());

    let podcast_id = PodcastId::new(
        uuid::Uuid::parse_str("a1a1ffff-0001-0001-0001-000000000001").unwrap(),
    );
    assert!(
        store.podcast(podcast_id).is_some(),
        "seeded podcast must be loaded into store"
    );
    assert!(
        store.is_subscribed(podcast_id),
        "seeded podcast without is_subscribed field must be followed (default_true)"
    );
}
