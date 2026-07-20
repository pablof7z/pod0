use pod0_domain::CommandId;
use rusqlite::Connection;

use crate::chapter_import_commit::commit_chapter_import_with_observer;
use crate::chapter_import_store_write::write_chapter_import;
use crate::chapter_import_test_support::{
    ChapterImportFixture, EPISODE_ID, FixedClock, IMPORT_ID, PODCAST_ID, STORE_ID,
};
use crate::chapter_legacy_backup::create_or_reuse_chapter_backups;
use crate::legacy_chapter_source::inspect_chapter_source;
use crate::legacy_format::finite_milliseconds;
use crate::{
    CURRENT_SCHEMA_VERSION, ChapterImportState, ChapterImporter, CoreStoreMigrator, MigrationClock,
    StorageError, read_chapter_import,
};

#[test]
fn seconds_to_milliseconds_rounds_and_rejects_invalid_ranges() {
    assert_eq!(finite_milliseconds(0.0, "chapter", 0).unwrap(), 0);
    assert_eq!(finite_milliseconds(0.000_4, "chapter", 0).unwrap(), 0);
    assert_eq!(finite_milliseconds(0.000_5, "chapter", 0).unwrap(), 1);
    for value in [f64::NAN, f64::INFINITY, -0.001, i64::MAX as f64 / 1_000.0] {
        assert!(matches!(
            finite_milliseconds(value, "chapter", 0),
            Err(StorageError::InvalidLegacyRecord { .. })
        ));
    }
}

#[test]
fn staged_evidence_tampering_marks_the_import_corrupt() {
    let fixture = valid_episode_fixture();
    fixture.stage(1_800_000_100_000);
    fixture
        .target_connection()
        .execute(
            "UPDATE pod0_chapter_import_entries SET raw_byte_count=raw_byte_count+1",
            [],
        )
        .unwrap();
    let error = ChapterImporter::new(FixedClock(1_800_000_100_001))
        .verify(
            &fixture.source,
            &fixture.artifacts,
            &fixture.legacy_backup,
            &fixture.target,
            IMPORT_ID,
        )
        .unwrap_err();
    assert_eq!(error.code(), "invalid_chapter_artifact");
    assert_eq!(
        read_chapter_import(&fixture.target, IMPORT_ID)
            .unwrap()
            .state,
        ChapterImportState::Corrupt
    );
}

#[test]
fn interrupted_stage_and_commit_recover_without_partial_history() {
    let fixture = valid_episode_fixture();
    let source = inspect_chapter_source(&fixture.source, &fixture.artifacts).unwrap();
    CoreStoreMigrator::new(MigrationFixedClock)
        .migrate(
            &fixture.target,
            CURRENT_SCHEMA_VERSION,
            &fixture.schema_backup,
            STORE_ID,
        )
        .unwrap();
    let backup =
        create_or_reuse_chapter_backups(&fixture.source, &fixture.legacy_backup, &source).unwrap();
    let interrupted = write_chapter_import(
        &fixture.target,
        IMPORT_ID,
        STORE_ID,
        &source,
        &backup,
        1_800_000_110_000,
        || Err(StorageError::Interrupted),
    );
    assert!(matches!(interrupted, Err(StorageError::Interrupted)));
    let connection = fixture.target_connection();
    let counts: (i64, i64) = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM pod0_chapter_imports),\
             (SELECT COUNT(*) FROM pod0_chapter_artifacts)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0));
    drop(connection);

    fixture.stage(1_800_000_110_001);
    fixture.verify(1_800_000_110_002);
    let interrupted = commit_chapter_import_with_observer(
        &fixture.source,
        &fixture.artifacts,
        &fixture.target,
        IMPORT_ID,
        1_800_000_110_003,
        || Err(StorageError::Interrupted),
    );
    assert!(matches!(interrupted, Err(StorageError::Interrupted)));
    let connection = fixture.target_connection();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM pod0_chapter_selections", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
    drop(connection);
    assert_eq!(
        read_chapter_import(&fixture.target, IMPORT_ID)
            .unwrap()
            .state,
        ChapterImportState::Verified
    );
    assert_eq!(
        fixture.import(1_800_000_110_004).state,
        ChapterImportState::Imported
    );
}

#[test]
fn command_reuse_with_a_different_fingerprint_is_rejected() {
    let fixture = valid_episode_fixture();
    let plan = fixture.inspect();
    fixture.stage(1_800_000_120_000);
    let error = ChapterImporter::new(FixedClock(1_800_000_120_001))
        .stage(
            &fixture.source,
            &fixture.artifacts,
            &fixture.legacy_backup,
            &fixture.target,
            &fixture.schema_backup,
            &plan,
            IMPORT_ID,
            CommandId::from_parts(7, 7),
        )
        .unwrap_err();
    assert!(matches!(error, StorageError::ChapterImportConflict));
}

#[test]
fn malformed_and_future_sources_fail_closed_without_fabricated_artifacts() {
    let malformed = ChapterImportFixture::new_v1();
    malformed.insert_episode(EPISODE_ID, PODCAST_ID, "{not-json");
    let plan = malformed.inspect();
    assert_eq!((plan.blocked_count, plan.canonical_artifact_count), (1, 0));
    malformed.stage(1_800_000_130_000);
    assert!(matches!(
        ChapterImporter::new(FixedClock(1_800_000_130_001)).verify(
            &malformed.source,
            &malformed.artifacts,
            &malformed.legacy_backup,
            &malformed.target,
            IMPORT_ID,
        ),
        Err(StorageError::InvalidChapterArtifact)
    ));
    assert_eq!(
        malformed
            .target_connection()
            .query_row("SELECT COUNT(*) FROM pod0_chapter_artifacts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );

    let future = ChapterImportFixture::new_v1();
    Connection::open(&future.source)
        .unwrap()
        .execute(
            "UPDATE workflow_schema_versions SET version=2 WHERE component='artifacts'",
            [],
        )
        .unwrap();
    assert!(matches!(
        crate::inspect_legacy_chapter_source(&future.source, &future.artifacts),
        Err(StorageError::NewerLegacyChapterSchema {
            stored: 2,
            supported: 1
        })
    ));
}

fn valid_episode_fixture() -> ChapterImportFixture {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        &crate::chapter_import_source_tests::episode_with_chapters(EPISODE_ID, true, false),
    );
    fixture
}

#[derive(Clone, Copy)]
struct MigrationFixedClock;

impl MigrationClock for MigrationFixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_100_000
    }
}
