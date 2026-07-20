use pod0_application::{
    ChapterObservationProjection, PublisherChapterObservation,
    qualify_publisher_chapter_observation,
};
use pod0_domain::{
    CancellationId, CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

use crate::listening_import_test_support::{
    EPISODE_ID, ImportFixture, create_sqlite_source, current_metadata, episode,
};
use crate::{
    LibraryStore, PublisherChapterEnsureOutcome, PublisherChapterWorkflowRecord,
    commit_listening_cutover,
};

pub(super) const SOURCE_URL: &str = "https://example.test/chapters.json";
const SOURCE_VERSION: &str = "source-v1";

pub(super) fn workflow_fixture() -> (ImportFixture, LibraryStore, EpisodeId) {
    let fixture = ImportFixture::new();
    create_sqlite_source(
        &fixture.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    fixture.stage(&fixture.plan()).unwrap();
    commit_listening_cutover(&fixture.target, 1_003).unwrap();
    let chapter_import_id = CommandId::from_parts(9, 9);
    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    connection
        .execute(
            "INSERT INTO pod0_chapter_imports(import_id,source_kind,source_identity,\
             source_generation,source_byte_count,source_database_digest,\
             source_selection_digest,command_fingerprint,evidence_count,artifact_count,\
             selected_count,blocked_count,chapter_count,ad_span_count,target_revision,state,\
             backup_database_digest,backup_database_byte_count,backup_file_count,\
             backup_file_byte_count,staged_at_ms,verified_at_ms,imported_at_ms) \
             VALUES(?1,'artifact_sqlite_v1',zeroblob(32),0,0,zeroblob(32),zeroblob(32),\
             zeroblob(32),0,0,0,0,0,0,1,'imported',zeroblob(32),0,0,0,1000,1000,1000)",
            [chapter_import_id.into_bytes().as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE pod0_chapter_state SET authority_active=1,authority_import_id=?1 \
             WHERE singleton=1",
            [chapter_import_id.into_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);
    let episode_id = EpisodeId::from_bytes([0x22; 16]);
    set_chapter_source(&fixture, episode_id, SOURCE_URL);
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    (fixture, store, episode_id)
}

pub(super) fn set_chapter_source(fixture: &ImportFixture, episode_id: EpisodeId, source_url: &str) {
    set_optional_chapter_source(fixture, episode_id, Some(source_url));
}

pub(super) fn set_optional_chapter_source(
    fixture: &ImportFixture,
    episode_id: EpisodeId,
    source_url: Option<&str>,
) {
    rusqlite::Connection::open(&fixture.target)
        .unwrap()
        .execute(
            "INSERT INTO pod0_episode_feed_metadata(episode_id,chapters_url,\
             persons_json,sound_bites_json) VALUES(?1,?2,'[]','[]') \
             ON CONFLICT(episode_id) DO UPDATE SET chapters_url=excluded.chapters_url",
            rusqlite::params![episode_id.into_bytes().as_slice(), source_url],
        )
        .unwrap();
}

pub(super) fn current_publisher_artifact(
    episode_id: EpisodeId,
) -> pod0_domain::ChapterArtifactInput {
    let mut artifact = publisher_artifact(episode_id);
    artifact.source_revision = SOURCE_VERSION.to_owned();
    artifact
}

pub(super) fn ensure(
    store: &LibraryStore,
    episode_id: EpisodeId,
    command: u64,
    force_retry: bool,
) -> PublisherChapterWorkflowRecord {
    let outcome = store
        .ensure_publisher_chapter_workflow(
            episode_id,
            SOURCE_URL,
            SOURCE_VERSION,
            CommandId::from_parts(1, command),
            CancellationId::from_parts(2, command),
            StateRevision::new(command),
            1_000 + i64::try_from(command).unwrap(),
            31_000 + i64::try_from(command).unwrap(),
            5,
            force_retry,
        )
        .unwrap();
    match outcome {
        PublisherChapterEnsureOutcome::Requested { record, .. }
        | PublisherChapterEnsureOutcome::Existing(record) => record,
    }
}

pub(super) fn publisher_artifact(episode_id: EpisodeId) -> pod0_domain::ChapterArtifactInput {
    let payload = br#"{"version":"1.2.0","chapters":[
      {"startTime":0,"title":"Opening"},
      {"startTime":60,"title":"Deep dive"}
    ]}"#
    .to_vec();
    let projection = qualify_publisher_chapter_observation(PublisherChapterObservation {
        episode_id,
        podcast_id: pod0_domain::PodcastId::from_bytes([0x11; 16]),
        resolved_source_url: SOURCE_URL.to_owned(),
        content_type: "application/json".to_owned(),
        payload_digest: ContentDigest::from_bytes(Sha256::digest(&payload).into()),
        payload,
        generated_at: UnixTimestampMilliseconds::new(1_100),
        duration_milliseconds: Some(120_000),
    });
    let ChapterObservationProjection::Qualified { artifact, .. } = projection else {
        panic!("fixture publisher document must qualify")
    };
    artifact
}

pub(super) fn failure(
    request_id: pod0_domain::HostRequestId,
    failure_code: &str,
    retry_at_ms: Option<i64>,
    retry_issued_revision: StateRevision,
    retry_deadline_at_ms: Option<i64>,
    observed_at_ms: i64,
) -> crate::PublisherChapterWorkflowFailureInput {
    crate::PublisherChapterWorkflowFailureInput {
        request_id,
        failure_code: failure_code.to_owned(),
        failure_detail: None,
        retry_at_ms,
        retry_issued_revision,
        retry_deadline_at_ms,
        observed_at_ms,
    }
}
