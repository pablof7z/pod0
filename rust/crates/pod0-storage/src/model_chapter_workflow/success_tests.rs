use pod0_application::{
    ChapterModelObservationMode, ChapterObservationProjection, ModelChapterObservation,
    qualify_model_chapter_observation,
};
use pod0_domain::{CancellationId, CommandId, PodcastId, StateRevision, UnixTimestampMilliseconds};

use super::tests::Fixture;
use super::*;
use crate::{CoreStoreMigrator, MigrationClock};

include!("success_test_clock.rs");

#[test]
fn qualified_commit_is_atomic_and_completion_history_survives_replan() {
    let fixture = Fixture::new();
    activate_chapter_authority(&fixture);
    let requested = fixture.ensure(7, None);
    let ModelChapterSubmissionClaim::Authorized(authorized) =
        fixture.claim(&requested, 1_800_000_100_010)
    else {
        panic!("claim must authorize")
    };
    let completion = fixture
        .store
        .stage_model_chapter_completion(fixture.completion(&authorized))
        .unwrap();
    let request = authorized.active_request.as_ref().unwrap();
    let projection = qualify_model_chapter_observation(ModelChapterObservation {
        episode_id: authorized.episode_id,
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
        completion_digest: completion.completion_digest,
        completion: completion.completion.clone(),
        generated_at: UnixTimestampMilliseconds::new(completion.generated_at_ms),
        duration_milliseconds: request.duration_ms,
        mode: ChapterModelObservationMode::Generate,
    });
    let ChapterObservationProjection::Qualified { artifact, .. } = projection else {
        panic!("persisted completion must qualify")
    };
    let success_input = ModelChapterSuccessInput {
        episode_id: authorized.episode_id,
        request_id: completion.request_id,
        generation: completion.generation,
        submission_fence_id: completion.submission_fence_id,
        artifact,
        completed_at_ms: 1_800_000_100_102,
    };
    let receipt = fixture
        .store
        .complete_model_chapter_workflow(success_input.clone())
        .unwrap();
    assert_eq!(receipt.workflow.state, ModelChapterWorkflowState::Succeeded);
    assert_eq!(
        receipt.workflow.selected_artifact_id,
        Some(receipt.chapter.artifact_id)
    );
    assert_eq!(
        fixture
            .store
            .complete_model_chapter_workflow(success_input)
            .unwrap(),
        receipt
    );

    let mut next_request = fixture.request(8);
    next_request.expected_selection_revision = receipt.chapter.selection_revision;
    let outcome = fixture
        .store
        .ensure_model_chapter_workflow(ModelChapterEnsureInput {
            episode_id: fixture.episode_id,
            configured_model: next_request.configured_model.clone(),
            desired_plan: ModelChapterDesiredPlan::Ready(Box::new(next_request)),
            command_id: CommandId::from_parts(40, 8),
            cancellation_id: CancellationId::from_parts(41, 8),
            issued_revision: StateRevision::new(8),
            now_ms: 1_800_000_100_110,
            request_deadline_ms: 1_800_000_160_110,
            max_attempts: 4,
            force_retry_from_revision: None,
        })
        .unwrap();
    let ModelChapterEnsureOutcome::Changed { record: next, .. } = outcome else {
        panic!("new fingerprint must create a new generation")
    };
    assert_eq!(next.generation, 2);
    assert_ne!(next.request_id, receipt.workflow.request_id);
    assert_eq!(
        fixture
            .store
            .model_chapter_completion(completion.request_id)
            .unwrap(),
        Some(completion),
        "raw completion evidence must outlive the current workflow row"
    );
}

fn activate_chapter_authority(fixture: &Fixture) {
    let import_id = CommandId::from_parts(90, 1);
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

#[test]
fn schema_16_completion_is_preserved_and_no_longer_pins_the_current_request() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(9, None);
    let ModelChapterSubmissionClaim::Authorized(authorized) =
        fixture.claim(&requested, 1_800_000_100_020)
    else {
        panic!("claim must authorize")
    };
    let completion = fixture
        .store
        .stage_model_chapter_completion(fixture.completion(&authorized))
        .unwrap();
    let path = fixture._transcript.import.target.clone();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys=OFF;
             DROP TABLE pod0_feed_apply_receipts;
             DROP TABLE pod0_feed_discovery_effects;
             DROP TABLE pod0_feed_discovery_workflows;
             DROP TABLE pod0_new_episode_notification_settings;
             DROP TABLE pod0_feed_discovery_items;
             DROP TABLE pod0_feed_discovery_occurrences;
             DROP TABLE pod0_compiled_memory_sources;
             DROP TABLE pod0_compiled_memory;
             DROP TABLE pod0_memories;
             DROP TABLE pod0_memory_cutover_evidence;
             DROP TABLE pod0_memory_state;
             DROP TABLE pod0_agent_history_staged_turns;
             DROP TABLE pod0_agent_history_staged_conversations;
             DROP TABLE pod0_agent_history_cutover_evidence;
             DROP TABLE pod0_agent_conversation_metadata;
             DROP TABLE pod0_publication_commands;
             DROP TABLE pod0_publication_facts;
             DROP TABLE pod0_signer_state;
             DROP TABLE pod0_publications;
             DROP TABLE pod0_agent_generated_audio_artifacts;
             DROP TABLE pod0_agent_audit;
             DROP TABLE pod0_agent_command_receipts;
             DROP TABLE pod0_agent_turns;
             DROP TABLE pod0_scheduled_completion_evidence;
             DROP TABLE pod0_generated_artifacts;
             DROP TABLE pod0_scheduled_command_receipts;
             DROP TABLE pod0_scheduled_attempts;
             DROP TABLE pod0_scheduled_occurrences;
             DROP TABLE pod0_scheduled_tasks;
             DROP TABLE pod0_scheduled_agent_cutover_evidence;
             DROP TABLE pod0_scheduled_agent_authority;
             DROP TABLE pod0_transcript_evidence_requests;
             DROP TABLE pod0_transcript_attempts;
             DROP TABLE pod0_transcript_workflows;
             DROP TABLE pod0_transcript_workflow_import_rows;
             DROP TABLE pod0_transcript_workflow_imports;
             DROP TABLE pod0_download_host_requests;
             DROP TABLE pod0_download_attempts;
             DROP TABLE pod0_download_workflows;
             DROP TABLE pod0_download_environment;
             DROP TABLE pod0_recall_configuration;
             ALTER TABLE pod0_model_chapter_completions
                 RENAME TO pod0_model_chapter_completions_v17;
             CREATE TABLE pod0_model_chapter_completions(
                 request_id BLOB PRIMARY KEY NOT NULL CHECK(length(request_id)=16),
                 episode_id BLOB NOT NULL REFERENCES pod0_model_chapter_workflows(episode_id)
                     ON DELETE CASCADE,
                 generation INTEGER NOT NULL CHECK(generation >= 1),
                 submission_fence_id BLOB NOT NULL CHECK(length(submission_fence_id)=16),
                 completion TEXT NOT NULL CHECK(length(CAST(completion AS BLOB)) <= 1048576),
                 completion_digest BLOB NOT NULL CHECK(length(completion_digest)=32),
                 provider TEXT NOT NULL, model TEXT NOT NULL,
                 prompt_tokens INTEGER, completion_tokens INTEGER, cached_tokens INTEGER,
                 reasoning_tokens INTEGER, cost_microusd INTEGER,
                 provider_operation_id TEXT, provider_status TEXT,
                 generated_at_ms INTEGER NOT NULL, observed_at_ms INTEGER NOT NULL,
                 FOREIGN KEY(episode_id,request_id,submission_fence_id)
                     REFERENCES pod0_model_chapter_workflows(
                         episode_id,request_id,submission_fence_id
                     ) ON DELETE CASCADE
             ) STRICT;
             INSERT INTO pod0_model_chapter_completions SELECT *
                 FROM pod0_model_chapter_completions_v17;
             DROP TABLE pod0_model_chapter_completions_v17;
             UPDATE pod0_schema_versions SET version=16 WHERE component='kernel';
             PRAGMA user_version=16;",
        )
        .unwrap();
    drop(connection);

    let report = CoreStoreMigrator::new(FixedClock)
        .migrate(
            &path,
            crate::CURRENT_SCHEMA_VERSION,
            &path.with_extension("v16-model-completion-backup.sqlite"),
            CommandId::from_parts(91, 17),
        )
        .unwrap();
    assert_eq!(
        report.applied_versions,
        [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30]
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute("PRAGMA foreign_keys=ON", []).unwrap();
    assert_eq!(
        connection
            .execute(
                "UPDATE pod0_model_chapter_workflows SET state='requested',generation=2,\
                 workflow_revision=workflow_revision+1,request_id=?1,submission_fence_id=?2,\
                 deadline_at_ms=?3,submission_authorized_at_ms=NULL,provider_operation_id=NULL,\
                 provider_status=NULL,may_have_submitted=0,updated_at_ms=?4 WHERE episode_id=?5",
                rusqlite::params![
                    [0xa1_u8; 16].as_slice(),
                    [0xa2_u8; 16].as_slice(),
                    1_800_000_260_000_i64,
                    1_800_000_200_001_i64,
                    fixture.episode_id.into_bytes().as_slice(),
                ],
            )
            .unwrap(),
        1
    );
    drop(connection);
    let reopened = crate::LibraryStore::open_authoritative(&path).unwrap();
    assert_eq!(
        reopened
            .model_chapter_completion(completion.request_id)
            .unwrap(),
        Some(completion)
    );
}

#[test]
fn post_claim_replan_requires_durable_completion_evidence() {
    let fixture = Fixture::new();
    let requested = fixture.ensure(10, None);
    let ModelChapterSubmissionClaim::Authorized(authorized) =
        fixture.claim(&requested, 1_800_000_100_030)
    else {
        panic!("claim must authorize")
    };
    assert_eq!(
        fixture
            .store
            .fail_model_chapter_workflow(ModelChapterFailureInput {
                episode_id: authorized.episode_id,
                request_id: authorized.request_id.unwrap(),
                generation: authorized.generation,
                submission_fence_id: authorized.submission_fence_id.unwrap(),
                failure_code: "stale_transcript".to_owned(),
                failure_detail: None,
                may_have_submitted: true,
                disposition: ModelChapterFailureDisposition::Replan,
                observed_at_ms: 1_800_000_100_031,
            }),
        Err(crate::StorageError::ChapterWorkflowConflict)
    );
    assert_eq!(
        fixture
            .store
            .model_chapter_workflow(fixture.episode_id)
            .unwrap()
            .unwrap()
            .state,
        ModelChapterWorkflowState::SubmissionAuthorized
    );
}
