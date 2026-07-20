use rusqlite::Connection;

use crate::chapter_import_commit::commit_chapter_import_with_observer;
use crate::chapter_import_test_support::{
    ChapterImportFixture, EPISODE_ID, FixedClock, IMPORT_ID, PODCAST_ID, STORE_ID,
};
use crate::{ChapterImportState, ChapterImporter, StorageError, read_chapter_import};

#[test]
fn source_change_inside_stage_fence_rolls_back_all_import_rows() {
    let fixture = valid_fixture();
    let plan = fixture.inspect();
    let result = ChapterImporter::new(FixedClock(1_800_000_140_000)).stage_with_observer(
        &fixture.source,
        &fixture.artifacts,
        &fixture.legacy_backup,
        &fixture.target,
        &fixture.schema_backup,
        &plan,
        IMPORT_ID,
        STORE_ID,
        || {
            Connection::open(&fixture.source)
                .unwrap()
                .execute(
                    "UPDATE persistence_metadata SET value='8' WHERE key='generation'",
                    [],
                )
                .unwrap();
            Ok(())
        },
    );
    assert!(matches!(result, Err(StorageError::SourceChanged)));
    let connection = fixture.target_connection();
    let counts: (i64, i64, i64) = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM pod0_chapter_imports),\
             (SELECT COUNT(*) FROM pod0_chapter_artifacts),\
             (SELECT COUNT(*) FROM pod0_chapter_import_entries)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[test]
fn source_change_inside_commit_fence_marks_corrupt_without_selection() {
    let fixture = valid_fixture();
    fixture.stage(1_800_000_150_000);
    fixture.verify(1_800_000_150_001);
    let result = commit_chapter_import_with_observer(
        &fixture.source,
        &fixture.artifacts,
        &fixture.target,
        IMPORT_ID,
        1_800_000_150_002,
        || {
            Connection::open(&fixture.source)
                .unwrap()
                .execute(
                    "UPDATE persistence_metadata SET value='8' WHERE key='generation'",
                    [],
                )
                .unwrap();
            Ok(())
        },
    );
    assert!(matches!(result, Err(StorageError::SourceChanged)));
    assert_eq!(
        read_chapter_import(&fixture.target, IMPORT_ID)
            .unwrap()
            .state,
        ChapterImportState::Corrupt
    );
    assert_eq!(
        fixture
            .target_connection()
            .query_row("SELECT COUNT(*) FROM pod0_chapter_selections", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}

#[test]
fn interruption_before_verify_commit_stays_staged_and_retries() {
    let fixture = valid_fixture();
    fixture.stage(1_800_000_151_000);
    let result = ChapterImporter::new(FixedClock(1_800_000_151_001)).verify_with_observer(
        &fixture.source,
        &fixture.artifacts,
        &fixture.legacy_backup,
        &fixture.target,
        IMPORT_ID,
        || Err(StorageError::Interrupted),
    );
    assert!(matches!(result, Err(StorageError::Interrupted)));
    assert_eq!(
        read_chapter_import(&fixture.target, IMPORT_ID)
            .unwrap()
            .state,
        ChapterImportState::Staged
    );
    assert_eq!(
        fixture.verify(1_800_000_151_002).report.state,
        ChapterImportState::Verified
    );
}

fn valid_fixture() -> ChapterImportFixture {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        &crate::chapter_import_source_tests::episode_with_chapters(EPISODE_ID, true, false),
    );
    fixture
}
