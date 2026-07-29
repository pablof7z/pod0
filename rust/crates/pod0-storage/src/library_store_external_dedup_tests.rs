use pod0_domain::{FeedIdentityV1, PodcastId};

use crate::library_store_tests::imported_fixture;
use crate::listening_import_test_support::*;
use crate::{LibraryStore, commit_listening_cutover};

#[test]
fn external_episode_dedupes_on_publisher_guid_even_when_audio_url_differs() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let requested_podcast_id = PodcastId::from_parts(91, 1);
    let feed = FeedIdentityV1 {
        source_url: "https://external.test/other-feed".to_owned(),
        comparison_key: "https://external.test/other-feed".to_owned(),
    };

    let first = store
        .upsert_external_episode(
            id(15),
            &"f".repeat(64),
            requested_podcast_id,
            Some(feed.clone()),
            "External show",
            "https://cdn-a.external.test/audio.mp3",
            Some("episode-guid-123"),
            "External episode",
            "External description",
            1_800_000_000_015,
            None,
            None,
            None,
            1_800_000_000_015,
        )
        .unwrap();

    // A later share resolves through a different CDN mirror (different
    // audio_url) but the same publisher guid — this must update the same
    // episode row, not create a second one under the same podcast.
    let second = store
        .upsert_external_episode(
            id(16),
            &"g".repeat(64),
            requested_podcast_id,
            Some(feed),
            "External show",
            "https://cdn-b.external.test/audio-mirror.mp3",
            Some("episode-guid-123"),
            "External episode",
            "External description",
            1_800_000_000_016,
            None,
            None,
            None,
            1_800_000_000_016,
        )
        .unwrap();

    assert_eq!(first.2, second.2);
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .episodes
            .iter()
            .filter(|episode| episode.podcast_id == requested_podcast_id)
            .count(),
        1
    );
    let episode = snapshot
        .episodes
        .iter()
        .find(|episode| episode.episode_id == first.2)
        .unwrap();
    assert_eq!(episode.publisher_guid, "episode-guid-123");
    assert_eq!(episode.enclosure_url, "https://cdn-b.external.test/audio-mirror.mp3");
}
