use pod0_domain::CommandId;

use crate::CoreStoreMigrator;
use crate::scheduled_agent_store_test_support::FixedClock;

#[test]
fn scheduled_agent_upgrade_is_one_step_and_starts_inactive() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("core.sqlite");
    let backup = directory.path().join("backup.sqlite");
    let migrator = CoreStoreMigrator::new(FixedClock);
    migrator
        .migrate(&store, 20, &backup, CommandId::from_parts(70, 1))
        .unwrap();

    let report = migrator
        .migrate(&store, 21, &backup, CommandId::from_parts(70, 2))
        .unwrap();

    assert_eq!(report.applied_versions, [21]);
    assert_eq!(report.backup.unwrap().schema_version, 20);
    let connection = rusqlite::Connection::open(store).unwrap();
    let authority: (String, i64, Option<i64>, Option<i64>) = connection
        .query_row(
            "SELECT state,core_revision,source_generation,committed_at_ms \
             FROM pod0_scheduled_agent_authority WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(authority, ("inactive".to_owned(), 0, None, None));
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN(\
             'pod0_scheduled_tasks','pod0_scheduled_occurrences','pod0_scheduled_attempts',\
             'pod0_scheduled_completion_evidence','pod0_generated_artifacts',\
             'pod0_scheduled_command_receipts')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 6);
}
