use pod0_domain::{CommandId, EpisodeId};
use rusqlite::{Connection, params};

use crate::feed_discovery_store_test_support::*;
use crate::listening_import_test_support::id;
use crate::{CURRENT_SCHEMA_VERSION, CoreStoreMigrator, LibraryStore, StorageError};

#[test]
fn schema_upgrade_foreign_keys_corruption_and_future_versions_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let backup = directory.path().join("backup.sqlite");
    let migrator = CoreStoreMigrator::new(FixedMigrationClock);
    migrator
        .migrate(&path, 28, &backup, CommandId::from_parts(4, 1))
        .unwrap();
    let report = migrator
        .migrate(
            &path,
            CURRENT_SCHEMA_VERSION,
            &backup,
            CommandId::from_parts(4, 2),
        )
        .unwrap();
    assert_eq!(report.applied_versions, [29]);
    assert_eq!(report.backup.unwrap().schema_version, 28);

    let (fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    store
        .apply_feed(
            id(130),
            &"1".repeat(64),
            podcast.clone(),
            vec![episode(podcast.podcast_id, 30, BASE_TIME)],
            false,
            true,
            None,
            None,
            BASE_TIME,
        )
        .unwrap();
    let occurrence = store.pending_feed_discoveries(1).unwrap().remove(0);
    drop(store);

    let connection = Connection::open(&fixture.target).unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    assert!(
        connection
            .execute(
                "INSERT INTO pod0_feed_discovery_items(
                    item_id,occurrence_id,episode_id,input_version,published_at_ms
                 ) VALUES(?1,?2,?3,?4,?5)",
                params![
                    [99_u8; 16].as_slice(),
                    occurrence.occurrence_id.into_bytes().as_slice(),
                    EpisodeId::from_parts(99, 99).into_bytes().as_slice(),
                    "9".repeat(64),
                    BASE_TIME,
                ],
            )
            .is_err()
    );
    connection
        .execute(
            "DELETE FROM pod0_feed_discovery_items WHERE occurrence_id=?1",
            [occurrence.occurrence_id.into_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);
    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert!(matches!(
        reopened.pending_feed_discoveries(1),
        Err(StorageError::CorruptSchema {
            detail: "feed discovery item count does not match"
        })
    ));
    drop(reopened);
    Connection::open(&fixture.target)
        .unwrap()
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
        .unwrap();
    assert!(matches!(
        LibraryStore::open_authoritative(&fixture.target),
        Err(StorageError::NewerSchema { .. })
    ));
}
