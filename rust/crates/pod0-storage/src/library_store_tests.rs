use pod0_domain::{
    CompletionStatus, DownloadArtifactStatus, EpisodeFeedMetadata, EpisodeId,
    EpisodeListeningState, EpisodeRecord, FeedIdentityV1, PodcastId, PodcastKind, PodcastRecord,
    TranscriptArtifactStatus, UnixTimestampMilliseconds,
};

use crate::listening_import_test_support::*;
use crate::{LibraryStore, commit_listening_cutover};

#[test]
fn cutover_is_atomic_idempotent_and_required_for_runtime_writes() {
    let fixture = imported_fixture();
    assert!(matches!(
        LibraryStore::open_authoritative(&fixture.target),
        Err(crate::StorageError::CutoverNotAuthoritative)
    ));

    assert!(!commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap());
    assert!(commit_listening_cutover(&fixture.target, 1_800_000_000_001).unwrap());
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(store.snapshot().unwrap().subscriptions.len(), 1);
}

#[test]
fn feed_upsert_preserves_identity_and_user_state_while_replacing_metadata() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let before = store.snapshot().unwrap();
    let imported = before.episodes[0].clone();
    let podcast = refreshed_podcast(before.podcasts[0].podcast_id);
    let mut episode = refreshed_episode(podcast.podcast_id, "guid-1");
    episode.episode_id = EpisodeId::from_parts(9, 9);

    let applied = store
        .apply_feed(
            id(10),
            &"a".repeat(64),
            podcast,
            vec![episode],
            false,
            true,
            Some("fresh-etag".to_owned()),
            Some("today".to_owned()),
            1_800_000_000_010,
        )
        .unwrap();
    let after = store.snapshot().unwrap();
    let updated = &after.episodes[0];

    assert_eq!(applied.podcast_id, before.podcasts[0].podcast_id);
    assert_eq!(updated.episode_id, imported.episode_id);
    assert_eq!(updated.listening, imported.listening);
    assert_eq!(updated.download, imported.download);
    assert_eq!(updated.title, "Fresh episode title");
    assert_eq!(after.podcasts[0].etag.as_deref(), Some("fresh-etag"));
    assert_eq!(after.playback.revision, applied.revision);

    let replay = store
        .apply_feed(
            id(10),
            &"a".repeat(64),
            refreshed_podcast(applied.podcast_id),
            vec![refreshed_episode(applied.podcast_id, "guid-1")],
            false,
            true,
            None,
            None,
            1_800_000_000_020,
        )
        .unwrap();
    assert_eq!(replay, applied);
    assert_eq!(store.snapshot().unwrap(), after);
}

#[test]
fn unsubscribe_removes_the_complete_podcast_slice_once() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let podcast_id = store.snapshot().unwrap().podcasts[0].podcast_id;

    let revision = store
        .unsubscribe(id(11), &"b".repeat(64), podcast_id, 1_800_000_000_011)
        .unwrap();
    let empty = store.snapshot().unwrap();
    assert!(empty.podcasts.is_empty());
    assert!(empty.subscriptions.is_empty());
    assert!(empty.episodes.is_empty());
    assert_eq!(empty.playback.revision, revision);
    assert_eq!(
        store
            .unsubscribe(id(11), &"b".repeat(64), podcast_id, 1_800_000_000_012)
            .unwrap(),
        revision
    );
}

#[test]
fn external_episode_and_placeholder_are_durable_without_creating_a_subscription() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let requested_podcast_id = PodcastId::from_parts(90, 1);
    let feed = FeedIdentityV1 {
        source_url: "https://external.test/feed".to_owned(),
        comparison_key: "https://external.test/feed".to_owned(),
    };

    let first = store
        .upsert_external_episode(
            id(12),
            &"c".repeat(64),
            requested_podcast_id,
            Some(feed.clone()),
            "External show",
            "https://external.test/audio.mp3",
            "External episode",
            "External description",
            1_800_000_000_011,
            Some("audio/mpeg"),
            Some("https://external.test/art.jpg"),
            Some(42_000),
            1_800_000_000_012,
        )
        .unwrap();
    let after_first = store.snapshot().unwrap();
    let episode = after_first
        .episodes
        .iter()
        .find(|episode| episode.publisher_guid == "https://external.test/audio.mp3")
        .unwrap();
    assert_eq!(first.1, requested_podcast_id);
    assert_eq!(first.2, episode.episode_id);
    assert_eq!(episode.title, "External episode");
    assert_eq!(episode.description, "External description");
    assert_eq!(episode.enclosure_mime_type.as_deref(), Some("audio/mpeg"));
    assert_eq!(episode.duration_milliseconds, Some(42_000));
    assert_eq!(after_first.podcasts.len(), 2);
    assert_eq!(after_first.subscriptions.len(), 1);

    assert_eq!(
        store
            .upsert_external_episode(
                id(12),
                &"c".repeat(64),
                requested_podcast_id,
                Some(feed.clone()),
                "External show",
                "https://external.test/audio.mp3",
                "Ignored replay",
                "Ignored description",
                1_800_000_000_013,
                None,
                None,
                None,
                1_800_000_000_013,
            )
            .unwrap(),
        first
    );
    assert_eq!(store.snapshot().unwrap(), after_first);

    let second = store
        .upsert_external_episode(
            id(13),
            &"d".repeat(64),
            PodcastId::from_parts(90, 2),
            Some(feed),
            "Duplicate identity",
            "https://external.test/audio.mp3",
            "Retitled external episode",
            "Retitled description",
            1_800_000_000_014,
            None,
            None,
            None,
            1_800_000_000_014,
        )
        .unwrap();
    let after_second = store.snapshot().unwrap();
    let updated = after_second
        .episodes
        .iter()
        .find(|episode| episode.episode_id == first.2)
        .unwrap();
    assert_eq!(second.1, requested_podcast_id);
    assert_eq!(second.2, first.2);
    assert_eq!(updated.title, "Retitled external episode");
    assert_eq!(updated.description, "Retitled description");
    assert_eq!(updated.published_at, episode.published_at);
}

#[test]
fn episode_starred_state_is_owned_and_replayed_by_the_library_store() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let episode_id = store.snapshot().unwrap().episodes[0].episode_id;

    let revision = store
        .set_episode_starred(id(13), &"d".repeat(64), episode_id, true, 1_800_000_000_013)
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.episodes[0].is_starred);
    assert_eq!(snapshot.playback.revision, revision);
    assert_eq!(
        store
            .set_episode_starred(id(13), &"d".repeat(64), episode_id, true, 1_800_000_000_014,)
            .unwrap(),
        revision
    );
}

#[test]
fn listening_reset_clears_library_and_playback_but_preserves_authority() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();

    let revision = store
        .reset_listening_data(id(14), &"e".repeat(64), 1_800_000_000_014)
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.podcasts.is_empty());
    assert!(snapshot.subscriptions.is_empty());
    assert!(snapshot.episodes.is_empty());
    assert!(snapshot.playback.queue.is_empty());
    assert_eq!(snapshot.playback.active_episode_id, None);
    assert_eq!(snapshot.playback.revision, revision);
    assert!(LibraryStore::open_authoritative(&fixture.target).is_ok());
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

fn refreshed_podcast(podcast_id: PodcastId) -> PodcastRecord {
    PodcastRecord {
        podcast_id,
        kind: PodcastKind::Rss,
        feed_identity: Some(FeedIdentityV1 {
            source_url: "https://EXAMPLE.test/Feed".to_owned(),
            comparison_key: "https://example.test/feed".to_owned(),
        }),
        title: "Fresh show title".to_owned(),
        author: "Fresh author".to_owned(),
        image_url: None,
        description: "Fresh description".to_owned(),
        language: Some("en".to_owned()),
        categories: vec!["Technology".to_owned()],
        discovered_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        title_is_placeholder: false,
        last_refreshed_at: Some(UnixTimestampMilliseconds::new(1_800_000_000_000)),
        etag: None,
        last_modified: None,
    }
}

fn refreshed_episode(podcast_id: PodcastId, guid: &str) -> EpisodeRecord {
    EpisodeRecord {
        episode_id: EpisodeId::from_parts(8, 8),
        podcast_id,
        publisher_guid: guid.to_owned(),
        title: "Fresh episode title".to_owned(),
        description: "Fresh notes".to_owned(),
        published_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        duration_milliseconds: Some(300_000),
        enclosure_url: "https://example.test/fresh.mp3".to_owned(),
        enclosure_mime_type: Some("audio/mpeg".to_owned()),
        image_url: None,
        feed_metadata: EpisodeFeedMetadata::default(),
        listening: EpisodeListeningState {
            resume_position_milliseconds: 0,
            completion: CompletionStatus::InProgress,
        },
        is_starred: false,
        download: DownloadArtifactStatus::Unavailable,
        transcript: TranscriptArtifactStatus::Unavailable,
        generated_audio: None,
    }
}
