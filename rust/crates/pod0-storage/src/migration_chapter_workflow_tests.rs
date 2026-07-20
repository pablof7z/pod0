use pod0_domain::{CommandId, StateRevision};

use crate::chapter_workflow_test_support::{current_publisher_artifact, ensure, workflow_fixture};
use crate::{CoreStoreMigrator, MigrationClock, PublisherChapterWorkflowState};

#[derive(Clone, Copy)]
struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_000_000
    }
}

#[test]
fn schema_14_to_15_preserves_and_adopts_current_publisher_chapters() {
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
            "DROP TABLE pod0_publisher_chapter_workflows;
             UPDATE pod0_schema_versions SET version=14 WHERE component='kernel';
             PRAGMA user_version=14;",
        )
        .unwrap();

    CoreStoreMigrator::new(FixedClock)
        .migrate(
            &fixture.target,
            15,
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
