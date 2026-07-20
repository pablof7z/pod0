use std::fs;

use rusqlite::Connection;

use crate::chapter_import_source_tests::workflow_chapters;
use crate::chapter_import_test_support::{
    ChapterImportFixture, EPISODE_ID, FixedClock, IMPORT_ID, PODCAST_ID,
};
use crate::{ChapterImportState, ChapterImporter, StorageError, read_chapter_import};

#[test]
fn valid_orphan_workflow_artifact_gets_a_hidden_retained_parent() {
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
    assert_eq!((plan.blocked_count, plan.canonical_artifact_count), (0, 1));
    assert_eq!(plan.selected_count, 1);
    orphan.prepare_target_with_listening_import();
    orphan.stage(1_800_000_160_000);
    orphan.verify(1_800_000_160_001);
    orphan.import(1_800_000_160_002);
    let connection = orphan.target_connection();
    assert!(
        !connection
            .query_row(
                "SELECT library_visible FROM pod0_podcasts WHERE podcast_id=?1",
                [crate::retained_orphan_parent::retained_orphan_podcast_id()
                    .into_bytes()
                    .as_slice()],
                |row| row.get::<_, bool>(0),
            )
            .unwrap()
    );
    let selected = crate::chapter_store_read_selection::read_selected_chapter_artifact(
        &connection,
        pod0_domain::EpisodeId::from_bytes([0x11; 16]),
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected.artifact.chapters[0].title, "Orphan");
}

#[test]
fn hash_mismatched_workflow_row_remains_blocked_evidence() {
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
fn selected_missing_file_is_preserved_as_blocked_evidence_and_fails_closed() {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        &crate::chapter_import_source_tests::episode_without_adjunct(EPISODE_ID),
    );
    let path = fixture.insert_workflow_artifact(
        "chapters",
        EPISODE_ID,
        "missing-input",
        "missing-output",
        "generated",
        "available",
        1_800_000_162.0,
        true,
        &workflow_chapters("Missing", true),
    );
    fs::remove_file(path).unwrap();

    let plan = fixture.inspect();
    assert_eq!((plan.blocked_count, plan.canonical_artifact_count), (1, 0));
    fixture.stage(1_800_000_162_000);
    assert_eq!(
        fixture
            .target_connection()
            .query_row(
                "SELECT diagnostic_code FROM pod0_chapter_import_entries",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "workflow_file_missing"
    );
    assert!(matches!(
        ChapterImporter::new(FixedClock(1_800_000_162_001)).verify(
            &fixture.source,
            &fixture.artifacts,
            &fixture.legacy_backup,
            &fixture.target,
            IMPORT_ID,
        ),
        Err(StorageError::InvalidChapterArtifact)
    ));
}

#[test]
fn multiple_selected_files_for_one_episode_are_rejected_before_staging() {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        &crate::chapter_import_source_tests::episode_without_adjunct(EPISODE_ID),
    );
    for version in ["first", "second"] {
        fixture.insert_workflow_artifact(
            "chapters",
            EPISODE_ID,
            version,
            version,
            "generated",
            "available",
            1_800_000_163.0,
            true,
            &workflow_chapters(version, true),
        );
    }

    assert!(matches!(
        crate::inspect_legacy_chapter_source(&fixture.source, &fixture.artifacts),
        Err(StorageError::InvalidLegacyRecord {
            detail: "multiple artifacts are selected",
            ..
        })
    ));
    assert!(!fixture.target.exists());
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
