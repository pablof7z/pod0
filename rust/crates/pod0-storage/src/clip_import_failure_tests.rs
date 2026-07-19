use std::fs;

use rusqlite::Connection;

use crate::listening_import_test_support::*;
use crate::{
    CURRENT_SCHEMA_VERSION, ClipImportClock, ClipImporter, CoreStoreMigrator, MigrationClock,
    StorageError, commit_clip_cutover, inspect_legacy_clip_source,
};

struct ClipTestClock;

impl ClipImportClock for ClipTestClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_323_100_000
    }
}

impl MigrationClock for ClipTestClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_323_100_000
    }
}

#[test]
fn malformed_clip_json_fails_closed_before_creating_a_target() {
    let fixture = ImportFixture::new();
    fs::write(&fixture.source, b"{\"clips\":[").unwrap();

    assert!(matches!(
        inspect_legacy_clip_source(&fixture.source),
        Err(StorageError::InvalidLegacyRecord {
            entity: "clips metadata",
            ..
        })
    ));
    assert!(!fixture.target.exists());
}

#[test]
fn future_core_schema_is_preserved_and_rejected_before_clip_import() {
    let fixture = ImportFixture::new();
    let mut metadata = current_metadata(7);
    metadata["clips"] = serde_json::json!([{
        "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
        "episodeID": EPISODE_ID,
        "subscriptionID": PODCAST_ID,
        "startMs": 1_000,
        "endMs": 2_000,
        "transcriptText": "Future-safe frozen words",
        "source": "touch"
    }]);
    create_sqlite_source(&fixture.source, &metadata, &[episode(EPISODE_ID, "guid-1")]);
    let plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    CoreStoreMigrator::new(ClipTestClock)
        .migrate(
            &fixture.target,
            CURRENT_SCHEMA_VERSION,
            &fixture.target_backup,
            id(2),
        )
        .unwrap();
    let connection = Connection::open(&fixture.target).unwrap();
    connection
        .execute_batch(&format!(
            "CREATE TABLE future_clip_data(value TEXT); \
             INSERT INTO future_clip_data VALUES('keep'); \
             PRAGMA user_version={}",
            CURRENT_SCHEMA_VERSION + 1
        ))
        .unwrap();
    drop(connection);

    assert!(matches!(
        commit_clip_cutover(&fixture.source, &fixture.target, 1_721_323_100_001),
        Err(StorageError::CorruptSchema { .. })
    ));

    let error = ClipImporter::new(ClipTestClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("clips.backup.sqlite"),
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(5),
            id(4),
        )
        .unwrap_err();
    assert!(matches!(error, StorageError::NewerSchema { .. }));
    assert_eq!(
        Connection::open(&fixture.target)
            .unwrap()
            .query_row("SELECT value FROM future_clip_data", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}
