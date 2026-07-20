use tempfile::tempdir;

use crate::{RecallIndex, RecallIndexError};

#[test]
fn corrupt_disposable_index_is_rebuilt_without_touching_canonical_evidence() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("recall.sqlite");
    let canonical = directory.path().join("canonical-evidence.sqlite");
    std::fs::write(&path, b"not a sqlite database").unwrap();
    std::fs::write(&canonical, b"canonical evidence").unwrap();

    let index = RecallIndex::open(&path, 4).unwrap();

    assert_eq!(index.sqlite_vec_version().unwrap(), "v0.1.9");
    assert_eq!(std::fs::read(canonical).unwrap(), b"canonical evidence");
    assert_ne!(std::fs::read(path).unwrap(), b"not a sqlite database");
}

#[test]
fn newer_disposable_schema_is_never_destroyed_during_open() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("recall.sqlite");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE pod0_recall_index_metadata(
               singleton INTEGER PRIMARY KEY,
               schema_version INTEGER NOT NULL,
               dimensions INTEGER NOT NULL,
               owner TEXT NOT NULL,
               legacy_cutover_committed INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO pod0_recall_index_metadata VALUES(1,2,4,'rust',0);",
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        RecallIndex::open(&path, 4),
        Err(RecallIndexError::IncompatibleSchema)
    ));
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT schema_version FROM pod0_recall_index_metadata",
                [],
                |row| row.get::<_, u32>(0)
            )
            .unwrap(),
        2
    );
}
