use super::*;
use podcast_core::{Episode, Podcast, PodcastId};
use url::Url;
use uuid::Uuid;
fn fixture_episode(podcast_id: PodcastId, title: &str) -> Episode {
    Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        title,
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    )
}
#[test]
fn set_and_clear_owner_pubkey_hex_round_trip() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Owned");
    let id_str = podcast.id.0.to_string();
    store.subscribe(podcast, vec![]);
    store.set_owner_pubkey_hex(&id_str, "abc123".into());
    assert_eq!(
        store.podcast_by_id_str(&id_str).and_then(|p| p.owner_pubkey_hex.clone()),
        Some("abc123".into())
    );
    store.clear_owner_pubkey_hex(&id_str);
    assert_eq!(
        store.podcast_by_id_str(&id_str).and_then(|p| p.owner_pubkey_hex.clone()),
        None
    );
}
#[test]
fn set_owner_pubkey_hex_silently_ignores_unknown_podcast() {
    let mut store = PodcastStore::new();
    // No panic, no state change.
    store.set_owner_pubkey_hex("never-subscribed", "abc".into());
    store.clear_owner_pubkey_hex("never-subscribed");
    assert_eq!(store.podcast_count(), 0);
}
#[test]
fn episode_with_podcast_clone_returns_pair_for_known_episode() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Source");
    let pid = podcast.id;
    let ep = fixture_episode(pid, "Pilot");
    let eid_str = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);
    let (p_out, e_out) = store
        .episode_with_podcast_clone(&eid_str)
        .expect("found");
    assert_eq!(p_out.id, pid);
    assert_eq!(e_out.title, "Pilot");
}
#[test]
fn episode_with_podcast_clone_returns_none_for_unknown_episode() {
    let store = PodcastStore::new();
    assert!(store.episode_with_podcast_clone("no-such-episode").is_none());
}

#[test]
fn insert_synthetic_podcast_creates_feedless_row() {
    let mut store = PodcastStore::new();
    let id = Uuid::new_v4().to_string();
    let ok = store.insert_synthetic_podcast(
        &id,
        "Synth".into(),
        "Desc".into(),
        "Agent".into(),
        Some("https://img/a.png".into()),
        Some("en".into()),
        vec!["Tech".into()],
        podcast_core::NostrVisibility::Public,
    );
    assert!(ok);
    let p = store.podcast_by_id_str(&id).expect("row present");
    assert_eq!(p.title, "Synth");
    assert_eq!(p.author, "Agent");
    assert!(p.feed_url.is_none());
    assert_eq!(
        p.kind,
        podcast_core::PodcastKind::Synthetic
    );
    assert_eq!(p.image_url.as_ref().map(|u| u.as_str()), Some("https://img/a.png"));
    assert_eq!(p.categories, vec!["Tech".to_string()]);
}

#[test]
fn insert_synthetic_podcast_rejects_bad_uuid() {
    let mut store = PodcastStore::new();
    let ok = store.insert_synthetic_podcast(
        "not-a-uuid",
        "T".into(),
        String::new(),
        String::new(),
        None,
        None,
        vec![],
        podcast_core::NostrVisibility::Public,
    );
    assert!(!ok);
    assert_eq!(store.podcast_count(), 0);
}

#[test]
fn update_owned_metadata_partial_keeps_unset_fields() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Original");
    let id = podcast.id.0.to_string();
    store.subscribe(podcast, vec![]);
    let ok = store.update_owned_metadata(&id, Some("New".into()), None, None, None, None);
    assert!(ok);
    let p = store.podcast_by_id_str(&id).unwrap();
    assert_eq!(p.title, "New");
    // description untouched (was empty).
    assert_eq!(p.description, "");
}

#[test]
fn update_owned_metadata_applies_author_and_visibility() {
    // Anti-clobber guarantee: author + visibility must mutate the kernel row
    // so the next snapshot push does not revert a Swift-side edit.
    let mut store = PodcastStore::new();
    let mut podcast = Podcast::new("Show");
    podcast.author = "Old Author".into();
    podcast.nostr_visibility = podcast_core::NostrVisibility::Private;
    let id = podcast.id.0.to_string();
    store.subscribe(podcast, vec![]);
    let ok = store.update_owned_metadata(
        &id,
        None,
        None,
        Some("New Author".into()),
        None,
        Some(podcast_core::NostrVisibility::Public),
    );
    assert!(ok);
    let p = store.podcast_by_id_str(&id).unwrap();
    assert_eq!(p.author, "New Author");
    assert_eq!(p.nostr_visibility, podcast_core::NostrVisibility::Public);
}

#[test]
fn update_owned_metadata_ignores_unparseable_artwork() {
    let mut store = PodcastStore::new();
    let mut podcast = Podcast::new("Art");
    podcast.image_url = Some(Url::parse("https://old/a.png").unwrap());
    let id = podcast.id.0.to_string();
    store.subscribe(podcast, vec![]);
    store.update_owned_metadata(&id, None, None, None, Some("::::not a url".into()), None);
    let p = store.podcast_by_id_str(&id).unwrap();
    // Prior artwork preserved, not blanked.
    assert_eq!(p.image_url.as_ref().map(|u| u.as_str()), Some("https://old/a.png"));
}

#[test]
fn update_owned_metadata_returns_false_for_unknown() {
    let mut store = PodcastStore::new();
    assert!(!store.update_owned_metadata("nope", Some("x".into()), None, None, None, None));
}

#[test]
fn remove_podcast_and_episodes_clears_row_and_episodes() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("ToDelete");
    let pid = podcast.id;
    let id = pid.0.to_string();
    let ep = fixture_episode(pid, "Ep");
    store.subscribe(podcast, vec![ep]);
    assert_eq!(store.podcast_count(), 1);
    store.remove_podcast_and_episodes(&id);
    assert_eq!(store.podcast_count(), 0);
    assert!(store.podcast_by_id_str(&id).is_none());
    assert!(store.episodes_for(pid).is_empty());
}

