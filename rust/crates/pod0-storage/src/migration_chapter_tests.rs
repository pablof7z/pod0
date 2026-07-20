use pod0_domain::CommandId;

use crate::chapter_import_test_support::{ChapterImportFixture, EPISODE_ID, PODCAST_ID};
use crate::migration::{MigrationBoundary, MigrationObserver};
use crate::{
    CURRENT_SCHEMA_VERSION, ChapterImporter, CoreStoreMigrator, MigrationClock, StorageError,
    chapter_store_is_authoritative, verify_backup,
};

#[derive(Clone, Copy)]
struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_000_000
    }
}

#[test]
fn chapter_artifact_upgrade_is_one_step_and_keeps_authority_inactive() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("core.sqlite");
    let backup = directory.path().join("core.backup.sqlite");
    let migrator = CoreStoreMigrator::new(FixedClock);
    migrator
        .migrate(&store, 12, &backup, CommandId::from_parts(0, 1))
        .unwrap();

    let report = migrator
        .migrate(&store, 13, &backup, CommandId::from_parts(0, 2))
        .unwrap();

    assert_eq!(report.applied_versions, [13]);
    assert_eq!(report.backup.unwrap().schema_version, 12);
    assert_eq!(verify_backup(&backup).unwrap().schema_version, 12);
    let connection = rusqlite::Connection::open(store).unwrap();
    let state: (i64, i64, Option<Vec<u8>>) = connection
        .query_row(
            "SELECT collection_revision,authority_active,authority_import_id \
             FROM pod0_chapter_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(state, (0, 0, None));
    assert!(
        connection
            .execute(
                "UPDATE pod0_chapter_state SET authority_active=1 WHERE singleton=1",
                [],
            )
            .is_err()
    );
}

#[test]
fn chapter_schema_preserves_blocked_malformed_source_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("core.sqlite");
    let backup = directory.path().join("core.backup.sqlite");
    CoreStoreMigrator::new(FixedClock)
        .migrate(&store, 13, &backup, CommandId::from_parts(0, 1))
        .unwrap();
    let connection = rusqlite::Connection::open(store).unwrap();

    connection
        .execute_batch(
            "INSERT INTO pod0_chapter_imports(
                import_id,source_kind,source_identity,source_generation,source_byte_count,
                source_database_digest,source_selection_digest,command_fingerprint,
                evidence_count,artifact_count,selected_count,blocked_count,chapter_count,
                ad_span_count,target_revision,state,backup_database_digest,
                backup_database_byte_count,backup_file_count,backup_file_byte_count,
                staged_at_ms,diagnostic_code
             ) VALUES(
                X'10101010101010101010101010101010','artifact_sqlite_v1',
                zeroblob(32),0,12,zeroblob(32),zeroblob(32),
                X'1212121212121212121212121212121212121212121212121212121212121212',
                1,0,1,1,0,0,1,'corrupt',zeroblob(32),12,1,12,1,'source_invalid');
             INSERT INTO pod0_chapter_import_entries(
                import_id,entry_id,evidence_kind,source_kind,source_subject,
                source_schema_version,source_integrity,source_row_digest,source_file_path,
                source_file_digest,source_file_byte_count,raw_digest,raw_byte_count,
                backup_file_digest,backup_file_byte_count,legacy_selected,importer_selected,
                validation_state,diagnostic_code
             ) VALUES(
                X'10101010101010101010101010101010',
                zeroblob(32),'episode_adjunct','episode_adjunct',
                'malformed-subject',0,'blocked',zeroblob(32),'/legacy/episodes.json',
                zeroblob(32),12,zeroblob(32),12,zeroblob(32),12,NULL,1,
                'blocked','invalid_episode_id');",
        )
        .unwrap();

    type BlockedChapterEvidenceRow = (
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        Option<i64>,
        i64,
    );
    let evidence: BlockedChapterEvidenceRow = connection
        .query_row(
            "SELECT episode_id,podcast_id,artifact_id,legacy_selected,importer_selected \
                 FROM pod0_chapter_import_entries",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(evidence, (None, None, None, None, 1));
}

#[test]
fn interrupted_chapter_schema_step_rolls_back_and_resumes_cleanly() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("core.sqlite");
    let backup = directory.path().join("core.backup.sqlite");
    let migrator = CoreStoreMigrator::new(FixedClock);
    migrator
        .migrate(&store, 12, &backup, CommandId::from_parts(0, 1))
        .unwrap();

    let interrupted = migrator.migrate_with_observer(
        &store,
        13,
        &backup,
        CommandId::from_parts(0, 2),
        &InterruptChapterStep,
    );
    assert!(matches!(interrupted, Err(StorageError::Interrupted)));
    let connection = rusqlite::Connection::open(&store).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        12
    );
    assert!(
        !connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='pod0_chapter_state')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .unwrap()
    );
    drop(connection);

    let resumed = migrator
        .migrate(&store, 13, &backup, CommandId::from_parts(0, 3))
        .unwrap();
    assert!(resumed.resumed_from_journal);
    assert_eq!(resumed.applied_versions, [13]);
}

#[test]
fn schema_13_imported_history_revalidates_and_activates_after_upgrade() {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        r#"{
          "id":"11111111-1111-1111-1111-111111111111",
          "podcastID":"22222222-2222-2222-2222-222222222222",
          "pubDate":"2026-07-19T00:00:00Z",
          "duration":120.0,
          "chapters":[
            {"startTime":0.0,"title":"Preserved","includeInTableOfContents":true,
             "isAIGenerated":false}
          ],
          "adSegments":[]
        }"#,
    );
    fixture.stage(1_800_000_000_000);
    fixture.verify(1_800_000_000_001);
    fixture.import(1_800_000_000_002);

    fixture
        .target_connection()
        .execute_batch(
            "DROP TABLE pod0_chapter_commands;
             CREATE TABLE pod0_chapter_selections_v13(
               episode_id BLOB NOT NULL CHECK(length(episode_id)=16),
               selection_revision INTEGER NOT NULL CHECK(selection_revision>=1),
               artifact_id BLOB NOT NULL CHECK(length(artifact_id)=16),
               source_import_id BLOB NOT NULL CHECK(length(source_import_id)=16),
               selected_at_ms INTEGER NOT NULL CHECK(selected_at_ms>=0),
               PRIMARY KEY(episode_id,selection_revision),
               UNIQUE(episode_id,source_import_id),
               FOREIGN KEY(artifact_id,episode_id)
                 REFERENCES pod0_chapter_artifacts(artifact_id,episode_id),
               FOREIGN KEY(source_import_id) REFERENCES pod0_chapter_imports(import_id)
             ) STRICT;
             INSERT INTO pod0_chapter_selections_v13 SELECT * FROM pod0_chapter_selections;
             DROP TABLE pod0_chapter_selections;
             ALTER TABLE pod0_chapter_selections_v13 RENAME TO pod0_chapter_selections;
             CREATE INDEX pod0_chapter_selections_import_idx
               ON pod0_chapter_selections(source_import_id,selection_revision);
             CREATE TABLE pod0_chapter_state_v13(
               singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
               collection_revision INTEGER NOT NULL CHECK(collection_revision>=0),
               authority_active INTEGER NOT NULL CHECK(authority_active=0),
               authority_import_id BLOB REFERENCES pod0_chapter_imports(import_id),
               CHECK(authority_import_id IS NULL)
             ) STRICT;
             INSERT INTO pod0_chapter_state_v13 VALUES(1,1,0,NULL);
             DROP TABLE pod0_chapter_state;
             ALTER TABLE pod0_chapter_state_v13 RENAME TO pod0_chapter_state;
             UPDATE pod0_schema_versions SET version=13 WHERE component='kernel';
             PRAGMA user_version=13;",
        )
        .unwrap();

    let upgrade_backup = fixture.target.with_extension("upgrade-backup.sqlite");
    CoreStoreMigrator::new(FixedClock)
        .migrate(
            &fixture.target,
            CURRENT_SCHEMA_VERSION,
            &upgrade_backup,
            CommandId::from_parts(70, 1),
        )
        .unwrap();
    assert!(!chapter_store_is_authoritative(&fixture.target).unwrap());

    let report = ChapterImporter::new(crate::chapter_import_test_support::FixedClock(
        1_800_000_000_003,
    ))
    .commit(
        &fixture.source,
        &fixture.artifacts,
        &fixture.target,
        crate::chapter_import_test_support::IMPORT_ID,
    )
    .unwrap();
    assert_eq!(report.state, crate::ChapterImportState::Imported);
    assert!(chapter_store_is_authoritative(&fixture.target).unwrap());
    assert_eq!(
        fixture
            .target_connection()
            .query_row("SELECT COUNT(*) FROM pod0_chapter_selections", [], |row| {
                row.get::<_, u32>(0)
            })
            .unwrap(),
        1
    );
}

struct InterruptChapterStep;

impl MigrationObserver for InterruptChapterStep {
    fn reached(&self, boundary: MigrationBoundary) -> Result<(), StorageError> {
        if boundary == (MigrationBoundary::BeforeStepCommit { target: 13 }) {
            Err(StorageError::Interrupted)
        } else {
            Ok(())
        }
    }
}
