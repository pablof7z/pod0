use pod0_domain::{CancellationId, CommandId, StateRevision};

use crate::chapter_workflow_test_support::{current_publisher_artifact, ensure, workflow_fixture};
use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, MigrationClock, PublisherChapterWorkflowState,
};

#[derive(Clone, Copy)]
struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_000_000
    }
}

#[test]
fn schema_14_through_current_preserves_and_adopts_current_publisher_chapters() {
    let (fixture, store, episode_id) = workflow_fixture();
    store
        .commit_and_select_chapter(
            CommandId::from_parts(8, 1),
            StateRevision::INITIAL,
            current_publisher_artifact(episode_id),
            1_100,
        )
        .unwrap();
    drop(store);
    rusqlite::Connection::open(&fixture.target)
        .unwrap()
        .execute_batch(
            "DROP TABLE pod0_scheduled_completion_evidence;
             DROP TABLE pod0_generated_artifacts;
             DROP TABLE pod0_scheduled_command_receipts;
             DROP TABLE pod0_scheduled_attempts;
             DROP TABLE pod0_scheduled_occurrences;
             DROP TABLE pod0_scheduled_tasks;
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
             DROP TABLE pod0_model_chapter_completions;
             DROP TABLE pod0_model_chapter_workflows;
             DROP TABLE pod0_publisher_chapter_workflows;
             UPDATE pod0_schema_versions SET version=14 WHERE component='kernel';
             PRAGMA user_version=14;",
        )
        .unwrap();

    CoreStoreMigrator::new(FixedClock)
        .migrate(
            &fixture.target,
            CURRENT_SCHEMA_VERSION,
            &fixture.target.with_extension("v14-backup.sqlite"),
            CommandId::from_parts(70, 14),
        )
        .unwrap();
    let reopened = crate::LibraryStore::open_authoritative(&fixture.target).unwrap();
    let adopted = ensure(&reopened, episode_id, 2, false);

    assert_eq!(adopted.state, PublisherChapterWorkflowState::Succeeded);
    assert!(adopted.request_id.is_none());
    assert!(
        reopened
            .active_publisher_chapter_workflows(8)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn schema_15_to_current_preserves_publisher_state_and_adds_fenced_model_storage() {
    let (fixture, store, episode_id) = workflow_fixture();
    let publisher = ensure(&store, episode_id, 3, false);
    drop(store);
    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    connection
        .execute_batch(
            "DROP TABLE pod0_scheduled_completion_evidence;
             DROP TABLE pod0_generated_artifacts;
             DROP TABLE pod0_scheduled_command_receipts;
             DROP TABLE pod0_scheduled_attempts;
             DROP TABLE pod0_scheduled_occurrences;
             DROP TABLE pod0_scheduled_tasks;
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
             DROP TABLE pod0_model_chapter_completions;
             DROP TABLE pod0_model_chapter_workflows;
             UPDATE pod0_schema_versions SET version=15 WHERE component='kernel';
             PRAGMA user_version=15;",
        )
        .unwrap();
    drop(connection);

    let report = CoreStoreMigrator::new(FixedClock)
        .migrate(
            &fixture.target,
            CURRENT_SCHEMA_VERSION,
            &fixture.target.with_extension("v15-backup.sqlite"),
            CommandId::from_parts(70, 15),
        )
        .unwrap();
    assert_eq!(report.applied_versions, [16, 17, 18, 19, 20, 21]);

    let reopened = crate::LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(
        reopened
            .publisher_chapter_workflow(episode_id)
            .unwrap()
            .unwrap(),
        publisher
    );
    drop(reopened);

    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    connection.execute("PRAGMA foreign_keys=ON", []).unwrap();
    connection
        .execute(
            "INSERT INTO pod0_model_chapter_workflows(
                episode_id,state,desired_configured_model,replan_pending,generation,
                workflow_revision,attempt,max_attempts,command_id,cancellation_id,
                issued_revision,may_have_submitted,created_at_ms,updated_at_ms
             ) VALUES(?1,'awaiting_transcript','openai/gpt-4o-mini',0,0,1,0,3,?2,?3,0,0,100,100)",
            rusqlite::params![
                episode_id.into_bytes().as_slice(),
                CommandId::from_parts(16, 1).into_bytes().as_slice(),
                CancellationId::from_parts(16, 2).into_bytes().as_slice(),
            ],
        )
        .unwrap();
    assert!(
        connection
            .execute(
                "UPDATE pod0_model_chapter_workflows SET state='requested' WHERE episode_id=?1",
                [episode_id.into_bytes().as_slice()],
            )
            .is_err(),
        "requested state must not exist without its complete typed request and fences"
    );
}
