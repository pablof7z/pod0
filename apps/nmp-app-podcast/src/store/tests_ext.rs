//! Overflow tests for [`super::PodcastStore`].
//!
//! Split from `store/tests.rs` to keep both files under the AGENTS.md 500-line
//! hard ceiling. Covers agent-memory, persistence integration, and settings.
use super::tests::{make_episode, make_podcast, TempDir};
use super::*;
use podcast_core::PodcastId;

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
