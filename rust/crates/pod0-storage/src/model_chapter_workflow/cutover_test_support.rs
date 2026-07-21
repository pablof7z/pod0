use pod0_application::{
    ChapterModelObservationMode, ChapterObservationProjection, ModelChapterObservation,
    qualify_model_chapter_observation,
};
use pod0_domain::{
    CancellationId, CommandId, ContentDigest, PodcastId, StateRevision, UnixTimestampMilliseconds,
};
use sha2::Digest as _;

use super::tests::Fixture;
use super::*;

pub(super) const NOW: i64 = 1_800_000_300_000;

pub(super) fn cutover(
    source_generation: u64,
    entries: Vec<LegacyModelChapterWorkflowEntry>,
) -> LegacyModelChapterWorkflowCutoverInput {
    LegacyModelChapterWorkflowCutoverInput {
        source_generation,
        entries,
        command_id: CommandId::from_parts(70, source_generation),
        cancellation_id: CancellationId::from_parts(71, source_generation),
        issued_revision: StateRevision::new(7),
        now_ms: NOW,
        max_attempts: 8,
    }
}

pub(super) fn entry(
    fixture: &Fixture,
    request: StoredModelChapterRequest,
    disposition: LegacyModelChapterWorkflowDisposition,
) -> LegacyModelChapterWorkflowEntry {
    LegacyModelChapterWorkflowEntry {
        episode_id: fixture.episode_id,
        configured_model: request.configured_model.clone(),
        request,
        disposition,
    }
}

pub(super) fn qualified_artifact(
    fixture: &Fixture,
    request: &StoredModelChapterRequest,
) -> pod0_domain::ChapterArtifactInput {
    let completion = r#"{"chapters":[{"start":0,"title":"Opening"},{"start":30,"title":"Context"},{"start":60,"title":"Deep dive"},{"start":90,"title":"Close"}],"ads":[]}"#;
    let projection = qualify_model_chapter_observation(ModelChapterObservation {
        episode_id: fixture.episode_id,
        podcast_id: PodcastId::from_bytes([0x11; 16]),
        format_version: request.format_version,
        requested_transcript_version_id: request.requested_transcript_version_id,
        requested_transcript_content_digest: request.requested_transcript_digest,
        selected_transcript_version_id: request.selected_transcript_version_id,
        selected_transcript_content_digest: request.selected_transcript_digest,
        policy_version: request.policy_version,
        source_version: request.source_version.clone(),
        provider: request.provider.clone(),
        model: request.model.clone(),
        completion_digest: ContentDigest::from_bytes(sha2::Sha256::digest(completion).into()),
        completion: completion.into(),
        generated_at: UnixTimestampMilliseconds::new(NOW - 1),
        duration_milliseconds: request.duration_ms,
        mode: ChapterModelObservationMode::Generate,
    });
    let ChapterObservationProjection::Qualified { artifact, .. } = projection else {
        panic!("model fixture must qualify")
    };
    artifact
}

pub(super) fn activate_chapter_authority(fixture: &Fixture) {
    let import_id = CommandId::from_parts(90, 61);
    let connection = rusqlite::Connection::open(&fixture._transcript.import.target).unwrap();
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
            [import_id.into_bytes().as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE pod0_chapter_state SET authority_active=1,authority_import_id=?1 \
             WHERE singleton=1",
            [import_id.into_bytes().as_slice()],
        )
        .unwrap();
}
