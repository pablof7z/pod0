use std::fs;

use rusqlite::Connection;

use crate::chapter_import_source_tests::workflow_chapters;
use crate::chapter_import_test_support::{
    ChapterImportFixture, EPISODE_ID, FixedClock, IMPORT_ID, PODCAST_ID,
};
use crate::{ChapterImportState, ChapterImporter, StorageError, read_chapter_import};

#[test]
fn orphan_and_corrupt_workflow_rows_remain_blocked_evidence() {
    let orphan = ChapterImportFixture::new_v1();
    orphan.insert_workflow_artifact(
        "chapters",
        EPISODE_ID,
        "orphan-input",
        "orphan-output",
        "generated",
        "available",
        1_800_000_160.0,
        true,
        &workflow_chapters("Orphan", true),
    );
    let plan = orphan.inspect();
    assert_eq!((plan.blocked_count, plan.canonical_artifact_count), (1, 0));
    orphan.stage(1_800_000_160_000);
    assert_eq!(
        orphan
            .target_connection()
            .query_row(
                "SELECT COUNT(*) FROM pod0_chapter_import_entries \
                 WHERE validation_state='blocked' AND episode_id IS NOT NULL AND podcast_id IS NULL",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );

    let corrupt = ChapterImportFixture::new_v1();
    corrupt.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        &crate::chapter_import_source_tests::episode_without_adjunct(EPISODE_ID),
    );
    let path = corrupt.insert_workflow_artifact(
        "chapters",
        EPISODE_ID,
        "corrupt-input",
        "corrupt-output",
        "generated",
        "available",
        1_800_000_161.0,
        true,
        &workflow_chapters("Before", true),
    );
    fs::write(path, workflow_chapters("After", true)).unwrap();
    let plan = corrupt.inspect();
    assert_eq!((plan.blocked_count, plan.canonical_artifact_count), (1, 0));
    corrupt.stage(1_800_000_161_000);
    assert!(matches!(
        ChapterImporter::new(FixedClock(1_800_000_161_001)).verify(
            &corrupt.source,
            &corrupt.artifacts,
            &corrupt.legacy_backup,
            &corrupt.target,
            IMPORT_ID,
        ),
        Err(StorageError::InvalidChapterArtifact)
    ));
    assert_eq!(
        read_chapter_import(&corrupt.target, IMPORT_ID)
            .unwrap()
            .state,
        ChapterImportState::Corrupt
    );
}

#[test]
fn duplicated_workflow_identity_is_rejected_before_target_creation() {
    let fixture = ChapterImportFixture::new_v1();
    let connection = Connection::open(&fixture.source).unwrap();
    connection
        .execute_batch(
            "DROP TABLE artifacts;
         CREATE TABLE artifacts(
           id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,subject_id TEXT NOT NULL,
           input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,
           location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,
           verified_at REAL NOT NULL,selected INTEGER NOT NULL);
         INSERT INTO artifacts(kind,subject_id,input_version,output_version,content_hash,
           location,origin,schema_version,integrity,verified_at,selected)
         VALUES
           ('chapters','11111111-1111-1111-1111-111111111111','same','same','00',
            '/missing-a','generated',1,'stale',1,0),
           ('chapters','11111111-1111-1111-1111-111111111111','same','same','00',
            '/missing-b','generated',1,'stale',2,0);",
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        crate::inspect_legacy_chapter_source(&fixture.source, &fixture.artifacts),
        Err(StorageError::InvalidLegacyRecord {
            detail: "artifact identity is duplicated",
            ..
        })
    ));
    assert!(!fixture.target.exists());
}
