use pod0_domain::{
    CompletionStatus, DownloadArtifactStatus, EpisodeFeedMetadata, EpisodeId,
    EpisodeListeningState, EpisodeRecord, PodcastId, PodcastRecord, TranscriptArtifactStatus,
    UnixTimestampMilliseconds,
};

use crate::listening_import_test_support::{
    ImportFixture, create_sqlite_source, current_metadata,
};
use crate::{LibraryStore, MigrationClock, commit_listening_cutover};

pub(crate) const BASE_TIME: i64 = 1_800_000_000_000;

pub(crate) fn empty_authoritative_store() -> (ImportFixture, LibraryStore) {
    let fixture = ImportFixture::new();
    create_sqlite_source(&fixture.source, &current_metadata(1), &[]);
    fixture.stage(&fixture.plan()).unwrap();
    commit_listening_cutover(&fixture.target, BASE_TIME).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    (fixture, store)
}

pub(crate) fn podcast(store: &LibraryStore) -> PodcastRecord {
    store.snapshot().unwrap().podcasts.remove(0)
}

pub(crate) fn episode(
    podcast_id: PodcastId,
    value: u64,
    published_at_ms: i64,
) -> EpisodeRecord {
    EpisodeRecord {
        episode_id: EpisodeId::from_parts(8, value),
        podcast_id,
        publisher_guid: format!("feed-discovery-{value}"),
        title: format!("Episode {value}"),
        description: format!("Description {value}"),
        published_at: UnixTimestampMilliseconds::new(published_at_ms),
        duration_milliseconds: Some(60_000 + value),
        enclosure_url: format!("https://example.test/{value}.mp3"),
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

#[derive(Clone, Copy)]
pub(crate) struct FixedMigrationClock;

impl MigrationClock for FixedMigrationClock {
    fn now_milliseconds(&self) -> i64 {
        BASE_TIME
    }
}
