use pod0_domain::CommandId;

use crate::{CoreStoreMigrator, MigrationClock, StorageError};

#[derive(Clone, Copy)]
struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_000_000
    }
}

#[test]
fn complete_history_upgrade_preserves_v11_import_and_allows_multiple_episode_rows() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("core.sqlite");
    let backup = directory.path().join("core.backup.sqlite");
    let migrator = CoreStoreMigrator::new(FixedClock);
    migrator
        .migrate(&store, 11, &backup, CommandId::from_parts(0, 1))
        .unwrap();
    let connection = rusqlite::Connection::open(&store).unwrap();
    seed_v11_transcript_import(&connection);
    drop(connection);

    let report = migrator
        .migrate(&store, 12, &backup, CommandId::from_parts(0, 2))
        .unwrap();

    assert_eq!(report.applied_versions, [12]);
    assert_eq!(report.backup.unwrap().schema_version, 11);
    let connection = rusqlite::Connection::open(&store).unwrap();
    let counts: (u32, u32) = connection
        .query_row(
            "SELECT artifact_count,selected_count FROM pod0_transcript_imports",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1));
    let preserved: (i64, bool) = connection
        .query_row(
            "SELECT legacy_row_id,is_selected FROM pod0_transcript_import_entries",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(preserved, (41, true));

    insert_historical_artifact(&connection).unwrap();
    let history_counts: (u32, u32) = connection
        .query_row(
            "SELECT COUNT(*),SUM(is_selected) FROM pod0_transcript_import_entries",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(history_counts, (2, 1));
}

fn seed_v11_transcript_import(connection: &rusqlite::Connection) {
    connection
        .execute_batch(
            "INSERT INTO pod0_listening_imports VALUES(
                X'01010101010101010101010101010101',1,
                'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',0,
                1,0,1,1,1,'verified',1);
             INSERT INTO pod0_podcasts VALUES(
                X'02020202020202020202020202020202',2,NULL,NULL,NULL,
                'Podcast','','','',NULL,'[]',1,0,NULL,NULL,NULL,
                X'01010101010101010101010101010101',1);
             INSERT INTO pod0_episodes VALUES(
                X'03030303030303030303030303030303',
                X'02020202020202020202020202020202','episode','Episode','',0,NULL,
                'https://example.com/audio',NULL,NULL,0,1,NULL,NULL,0,1,NULL,NULL,NULL,NULL,
                1,NULL,NULL,NULL,NULL,NULL,X'7B7D',X'01010101010101010101010101010101');
             INSERT INTO pod0_transcript_imports VALUES(
                X'10101010101010101010101010101010','artifact_sqlite_v1',1,7,
                zeroblob(32),zeroblob(32),zeroblob(32),128,1,1,'committed',NULL,10,11,12,NULL);
             INSERT INTO pod0_transcript_documents VALUES(
                X'04040404040404040404040404040404',
                X'03030303030303030303030303030303',
                X'02020202020202020202020202020202','source-v1',zeroblob(32),1,NULL,NULL,
                zeroblob(32),0);
             INSERT INTO pod0_transcript_artifacts VALUES(
                X'05050505050505050505050505050505',
                X'04040404040404040404040404040404',
                X'03030303030303030303030303030303',1,zeroblob(32),'en',1,0,0,0,
                X'10101010101010101010101010101010',1);
             INSERT INTO pod0_transcript_import_entries VALUES(
                X'10101010101010101010101010101010',
                X'03030303030303030303030303030303',41,1,'source-v1','output-v1',
                'publisher','available',1,zeroblob(32),zeroblob(32),zeroblob(32),12,
                X'05050505050505050505050505050505',
                X'04040404040404040404040404040404');",
        )
        .unwrap();
}

fn insert_historical_artifact(connection: &rusqlite::Connection) -> Result<(), StorageError> {
    connection
        .execute_batch(
            "INSERT INTO pod0_transcript_documents VALUES(
                X'06060606060606060606060606060606',
                X'03030303030303030303030303030303',
                X'02020202020202020202020202020202','source-v0',zeroblob(32),1,NULL,NULL,
                zeroblob(32),0);
             INSERT INTO pod0_transcript_artifacts VALUES(
                X'07070707070707070707070707070707',
                X'06060606060606060606060606060606',
                X'03030303030303030303030303030303',1,zeroblob(32),'en',1,0,0,0,
                X'10101010101010101010101010101010',1);
             INSERT INTO pod0_transcript_import_entries VALUES(
                X'10101010101010101010101010101010',
                X'03030303030303030303030303030303',42,1,'source-v0','output-v0',
                'publisher','available',1,0,zeroblob(32),zeroblob(32),zeroblob(32),12,
                X'07070707070707070707070707070707',
                X'06060606060606060606060606060606');
             UPDATE pod0_transcript_imports SET artifact_count=2;",
        )
        .map_err(|error| StorageError::sqlite("seed transcript history migration", error))
}
