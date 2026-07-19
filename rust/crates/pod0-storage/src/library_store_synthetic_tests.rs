use pod0_domain::{PodcastId, PodcastKind, PodcastRecord, UnixTimestampMilliseconds};

use crate::listening_import_test_support::*;
use crate::{LibraryStore, commit_listening_cutover};

#[test]
fn synthetic_podcast_and_local_episode_are_core_owned_without_a_feed() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let podcast_id = PodcastId::from_parts(91, 1);

    let (_, resolved, episode_id) = store
        .upsert_external_episode(
            id(14),
            &"e".repeat(64),
            podcast_id,
            None,
            "Unknown",
            "file:///tmp/generated.m4a",
            "Generated episode",
            "A generated briefing",
            1_800_000_000_020,
            Some("audio/mp4"),
            None,
            Some(12_000),
            1_800_000_000_021,
        )
        .unwrap();
    assert_eq!(resolved, podcast_id);
    let first = store.snapshot().unwrap();
    let parent = first
        .podcasts
        .iter()
        .find(|podcast| podcast.podcast_id == podcast_id)
        .unwrap();
    assert_eq!(parent.kind, PodcastKind::Synthetic);
    assert!(parent.feed_identity.is_none());
    assert!(
        first
            .subscriptions
            .iter()
            .all(|subscription| subscription.podcast_id != podcast_id)
    );
    assert_eq!(
        first
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
            .unwrap()
            .description,
        "A generated briefing"
    );

    store
        .upsert_synthetic_podcast(
            id(15),
            &"f".repeat(64),
            synthetic_podcast(podcast_id),
            1_800_000_000_022,
        )
        .unwrap();
    let updated = store.snapshot().unwrap();
    let parent = updated
        .podcasts
        .iter()
        .find(|podcast| podcast.podcast_id == podcast_id)
        .unwrap();
    assert_eq!(parent.title, "Agent Generated");
    assert_eq!(parent.categories, ["Briefings"]);
    assert_eq!(updated.episodes.len(), first.episodes.len());
}

fn synthetic_podcast(podcast_id: PodcastId) -> PodcastRecord {
    PodcastRecord {
        podcast_id,
        kind: PodcastKind::Synthetic,
        feed_identity: None,
        title: "Agent Generated".to_owned(),
        author: "Podcast Agent".to_owned(),
        image_url: Some("https://example.test/art.png".to_owned()),
        description: "Episodes made for this listener.".to_owned(),
        language: Some("en".to_owned()),
        categories: vec!["Briefings".to_owned()],
        discovered_at: UnixTimestampMilliseconds::new(1_800_000_000_022),
        title_is_placeholder: false,
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    }
}

fn imported_fixture() -> ImportFixture {
    let fixture = ImportFixture::new();
    create_sqlite_source(
        &fixture.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    fixture.stage(&fixture.plan()).unwrap();
    fixture
}
