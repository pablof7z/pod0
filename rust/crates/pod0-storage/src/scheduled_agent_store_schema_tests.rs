use rusqlite::Connection;

use crate::scheduled_agent_store_test_support::{FixedClock, ScheduledFixture, activate};
use crate::*;

#[test]
fn authority_starts_inactive_and_opens_only_after_explicit_cutover() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite");
    let backup = directory.path().join("backup.sqlite");
    CoreStoreMigrator::new(FixedClock)
        .migrate(
            &path,
            CURRENT_SCHEMA_VERSION,
            &backup,
            pod0_domain::CommandId::from_parts(1, 1),
        )
        .unwrap();
    assert_eq!(scheduled_agent_store_is_authoritative(&path), Ok(false));
    assert!(matches!(
        ScheduledAgentStore::open_authoritative(&path),
        Err(StorageError::CutoverNotAuthoritative)
    ));
    activate(&path);
    assert_eq!(scheduled_agent_store_is_authoritative(&path), Ok(true));
}

#[test]
fn every_supported_schema_version_upgrades_to_scheduled_storage() {
    for version in MIN_SUPPORTED_SCHEMA_VERSION..CURRENT_SCHEMA_VERSION {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("core.sqlite");
        let backup = directory.path().join("backup.sqlite");
        let migrator = CoreStoreMigrator::new(FixedClock);
        if version > 0 {
            migrator
                .migrate(
                    &path,
                    version,
                    &backup,
                    pod0_domain::CommandId::from_parts(2, u64::from(version)),
                )
                .unwrap();
        }
        let report = migrator
            .migrate(
                &path,
                CURRENT_SCHEMA_VERSION,
                &backup,
                pod0_domain::CommandId::from_parts(3, u64::from(version)),
            )
            .unwrap();
        assert_eq!(report.to_version, CURRENT_SCHEMA_VERSION);
        assert!(
            Connection::open(path)
                .unwrap()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master \
                     WHERE name='pod0_scheduled_attempts')",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap()
        );
    }
}

#[test]
fn future_and_corrupt_scheduled_schema_fail_closed() {
    let future = ScheduledFixture::new();
    Connection::open(&future.path)
        .unwrap()
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
        .unwrap();
    assert!(matches!(
        ScheduledAgentStore::open_authoritative(&future.path),
        Err(StorageError::NewerSchema { .. })
    ));

    let corrupt = ScheduledFixture::new();
    Connection::open(&corrupt.path)
        .unwrap()
        .execute("DROP TABLE pod0_scheduled_attempts", [])
        .unwrap();
    assert!(matches!(
        ScheduledAgentStore::open_authoritative(&corrupt.path),
        Err(StorageError::CorruptSchema { .. })
    ));
}
