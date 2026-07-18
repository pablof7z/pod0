use std::fs;

use rusqlite::Connection;

use crate::listening_import_test_support::*;
use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, ListeningImporter, MigrationClock, StorageError,
    inspect_legacy_listening_source,
};

#[test]
fn mismatched_or_aliased_verified_backup_is_rejected_before_target_commit() {
    let fixture = ImportFixture::new();
    create_sqlite_source(
        &fixture.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    let plan = fixture.plan();
    let other = fixture._directory.path().join("other.json");
    create_legacy_json(&other);
    fs::copy(&other, &fixture.source_backup).unwrap();
    assert_eq!(fixture.stage(&plan), Err(StorageError::BackupConflict));
    assert!(!fixture.target.exists());

    fs::remove_file(&fixture.source_backup).unwrap();
    let aliased = fixture
        ._directory
        .path()
        .join("new-directory")
        .join("..")
        .join("swift.sqlite");
    assert_eq!(
        ListeningImporter::new(FixedClock).stage(
            &fixture.source,
            &aliased,
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(1),
            id(2),
        ),
        Err(StorageError::BackupConflict)
    );
    assert_eq!(
        inspect_legacy_listening_source(&fixture.source).unwrap(),
        plan
    );
    assert!(!fixture.target.exists());
}

#[test]
fn changed_ambiguous_corrupt_and_newer_sources_fail_closed() {
    let changed = ImportFixture::new();
    create_sqlite_source(
        &changed.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    let stale_plan = changed.plan();
    let connection = Connection::open(&changed.source).unwrap();
    let payload: Vec<u8> = connection
        .query_row("SELECT payload FROM episodes", [], |row| row.get(0))
        .unwrap();
    let mut payload: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    payload["title"] = serde_json::json!("Changed title");
    connection
        .execute(
            "UPDATE episodes SET payload=?1",
            [serde_json::to_vec(&payload).unwrap()],
        )
        .unwrap();
    drop(connection);
    assert_eq!(changed.stage(&stale_plan), Err(StorageError::SourceChanged));
    assert!(!changed.target.exists());

    let ambiguous = ImportFixture::new();
    let mut metadata = current_metadata(7);
    let duplicate = metadata["podcasts"][0].clone();
    metadata["podcasts"].as_array_mut().unwrap().push(duplicate);
    create_sqlite_source(&ambiguous.source, &metadata, &[]);
    assert!(matches!(
        inspect_legacy_listening_source(&ambiguous.source),
        Err(StorageError::InvalidLegacyRecord { .. })
    ));

    let corrupt = ImportFixture::new();
    create_sqlite_source(
        &corrupt.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    Connection::open(&corrupt.source)
        .unwrap()
        .execute("UPDATE episodes SET guid='different'", [])
        .unwrap();
    assert!(matches!(
        inspect_legacy_listening_source(&corrupt.source),
        Err(StorageError::InvalidLegacyRecord { .. })
    ));

    let newer = ImportFixture::new();
    create_sqlite_source(
        &newer.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    CoreStoreMigrator::new(MigrationTestClock)
        .migrate(
            &newer.target,
            CURRENT_SCHEMA_VERSION,
            &newer.target_backup,
            id(2),
        )
        .unwrap();
    let connection = Connection::open(&newer.target).unwrap();
    connection.execute_batch(&format!("CREATE TABLE future_data(value TEXT); INSERT INTO future_data VALUES('keep'); PRAGMA user_version={}", CURRENT_SCHEMA_VERSION + 1)).unwrap();
    drop(connection);
    assert!(matches!(
        newer.stage(&newer.plan()),
        Err(StorageError::NewerSchema { .. })
    ));
    assert_eq!(
        Connection::open(&newer.target)
            .unwrap()
            .query_row("SELECT value FROM future_data", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}

struct MigrationTestClock;
impl MigrationClock for MigrationTestClock {
    fn now_milliseconds(&self) -> i64 {
        1
    }
}
