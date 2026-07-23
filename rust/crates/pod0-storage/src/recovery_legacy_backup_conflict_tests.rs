use rusqlite::Connection;

use crate::recovery_test_support::*;
use crate::*;

#[test]
fn legacy_reused_id_backup_conflict_retries_with_new_id_and_preserves_evidence() {
    let fixture = Fixture::new();
    fixture
        .migrator
        .migrate(&fixture.store, 2, &fixture.backup, id(1))
        .unwrap();
    let first_backup = fixture._directory.path().join("schema-backup-v3");
    fixture
        .migrator
        .migrate(&fixture.store, 3, &first_backup, id(1))
        .unwrap();

    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .execute(
            "INSERT INTO pod0_migration_journal( \
                migration_id,from_version,to_version,state,started_at_ms,completed_at_ms,diagnostic_code \
             ) VALUES(?1,3,4,'failed',10,10,'backup_conflict')",
            [id(1).into_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);

    assert_eq!(
        fixture.migrator.inspect(&fixture.store).migration_state,
        MigrationState::Blocked(BlockedReason::FailedMigration)
    );
    let retry_backup = fixture._directory.path().join("schema-backup-current");
    let report = fixture
        .migrator
        .migrate(&fixture.store, CURRENT_SCHEMA_VERSION, &retry_backup, id(2))
        .unwrap();
    assert!(report.resumed_from_journal);
    assert_eq!(report.from_version, 3);
    assert_eq!(report.to_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(verify_backup(&first_backup).unwrap().schema_version, 2);
    assert_eq!(verify_backup(&retry_backup).unwrap().schema_version, 3);

    let connection = Connection::open(&fixture.store).unwrap();
    let preserved_failure: (String, String) = connection
        .query_row(
            "SELECT state,diagnostic_code FROM pod0_migration_journal \
             WHERE migration_id=?1 AND to_version=4",
            [id(1).into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        preserved_failure,
        ("failed".to_owned(), "backup_conflict".to_owned())
    );
}

#[test]
fn legacy_backup_conflict_retry_requires_a_new_migration_id() {
    let fixture = Fixture::new();
    fixture
        .migrator
        .migrate(&fixture.store, 2, &fixture.backup, id(1))
        .unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .execute(
            "INSERT INTO pod0_migration_journal( \
                migration_id,from_version,to_version,state,started_at_ms,completed_at_ms,diagnostic_code \
             ) VALUES(?1,2,3,'failed',10,10,'backup_conflict')",
            [id(1).into_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);

    assert_eq!(
        fixture.migrator.migrate(
            &fixture.store,
            CURRENT_SCHEMA_VERSION,
            &fixture._directory.path().join("retry-backup"),
            id(1),
        ),
        Err(StorageError::FailedMigration { from: 2, to: 3 })
    );
}
