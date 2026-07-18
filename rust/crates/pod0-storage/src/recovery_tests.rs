use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;

use crate::migration::{MigrationBoundary, MigrationObserver};
use crate::recovery_test_support::*;
use crate::*;

struct FailOnce(AtomicBool);

impl MigrationObserver for FailOnce {
    fn reached(&self, boundary: MigrationBoundary) -> Result<(), StorageError> {
        if boundary == (MigrationBoundary::BeforeStepCommit { target: 3 })
            && !self.0.swap(true, Ordering::SeqCst)
        {
            Err(StorageError::Sqlite {
                operation: "injected migration failure",
            })
        } else {
            Ok(())
        }
    }
}

#[test]
fn corrupt_database_is_read_only_blocked_and_never_reset() {
    let fixture = Fixture::new();
    let original = b"not a sqlite database";
    fs::write(&fixture.store, original).unwrap();

    let status = fixture.migrator.inspect(&fixture.store);
    assert_eq!(status.access_mode, AccessMode::ReadOnlyRecovery);
    assert_eq!(
        status.migration_state,
        MigrationState::Blocked(BlockedReason::Corrupt)
    );
    assert!(fixture.migrate_to_current(1).is_err());
    assert_eq!(fs::read(&fixture.store).unwrap(), original);
    assert!(!fixture.backup.exists());
}

#[test]
fn newer_schema_and_unknown_current_schema_fail_closed() {
    let newer = Fixture::new();
    newer.migrate_to_current(1).unwrap();
    let connection = Connection::open(&newer.store).unwrap();
    connection
        .execute_batch(&format!(
            "CREATE TABLE future_data(value TEXT); \
             INSERT INTO future_data VALUES('keep-me'); \
             PRAGMA user_version={};",
            CURRENT_SCHEMA_VERSION + 1
        ))
        .unwrap();
    drop(connection);

    assert_eq!(
        newer.migrator.inspect(&newer.store).migration_state,
        MigrationState::Blocked(BlockedReason::NewerSchema)
    );
    assert!(matches!(
        newer.migrate_to_current(2),
        Err(StorageError::NewerSchema { .. })
    ));
    assert_eq!(
        Connection::open(&newer.store)
            .unwrap()
            .query_row("SELECT value FROM future_data", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "keep-me"
    );

    let unknown = Fixture::new();
    unknown
        .migrator
        .migrate(&unknown.store, 2, &unknown.backup, id(3))
        .unwrap();
    Connection::open(&unknown.store)
        .unwrap()
        .execute("DROP TABLE pod0_backup_evidence", [])
        .unwrap();
    assert_eq!(
        unknown.migrator.inspect(&unknown.store).migration_state,
        MigrationState::Blocked(BlockedReason::Corrupt)
    );
    assert!(matches!(
        unknown.migrate_to_current(4),
        Err(StorageError::CorruptSchema { .. })
    ));
}

#[test]
fn failed_migration_records_a_blocked_state_without_partial_schema() {
    let fixture = Fixture::new();
    fixture
        .migrator
        .migrate(&fixture.store, 2, &fixture.backup, id(1))
        .unwrap();
    let failure = FailOnce(AtomicBool::new(false));

    assert!(matches!(
        fixture
            .migrator
            .migrate_with_observer(&fixture.store, 3, &fixture.backup, id(2), &failure,),
        Err(StorageError::Sqlite { .. })
    ));
    assert_eq!(
        fixture.migrator.inspect(&fixture.store).migration_state,
        MigrationState::Blocked(BlockedReason::FailedMigration)
    );
    assert!(matches!(
        fixture.migrate_to_current(3),
        Err(StorageError::FailedMigration { from: 2, to: 3 })
    ));
    assert!(!table_exists(&fixture.store, "pod0_domain_cutovers"));
    assert_eq!(verify_backup(&fixture.backup).unwrap().schema_version, 2);
}

#[test]
fn verified_backup_restores_only_to_a_new_destination() {
    let fixture = Fixture::new();
    fixture
        .migrator
        .migrate(&fixture.store, 2, &fixture.backup, id(1))
        .unwrap();
    let report = fixture.migrate_to_current(2).unwrap();
    assert_eq!(
        report.backup.as_ref().map(|item| item.schema_version),
        Some(2)
    );

    let restored = fixture._directory.path().join("restored.sqlite");
    let evidence = restore_backup_to_new_store(&fixture.backup, &restored).unwrap();
    assert_eq!(evidence.schema_version, 2);
    assert_eq!(
        fixture.migrator.inspect(&restored).migration_state,
        MigrationState::Required {
            target_version: CURRENT_SCHEMA_VERSION
        }
    );
    assert_eq!(
        restore_backup_to_new_store(&fixture.backup, &restored),
        Err(StorageError::BackupConflict)
    );
}

#[test]
fn backup_path_must_not_alias_the_source_store() {
    let fixture = Fixture::new();
    fixture
        .migrator
        .migrate(&fixture.store, 2, &fixture.backup, id(1))
        .unwrap();
    let nested = fixture._directory.path().join("nested");
    fs::create_dir(&nested).unwrap();
    let aliased_source = nested.join("..").join("core.sqlite");

    assert_eq!(
        fixture
            .migrator
            .migrate(&fixture.store, 3, &aliased_source, id(2)),
        Err(StorageError::BackupConflict)
    );
    assert!(!fixture.backup.exists());
    assert_eq!(
        fixture.migrator.inspect(&fixture.store).migration_state,
        MigrationState::Required {
            target_version: CURRENT_SCHEMA_VERSION
        }
    );
}

#[test]
fn backup_from_another_store_is_never_reused() {
    let first = Fixture::new();
    first
        .migrator
        .migrate(&first.store, 2, &first.backup, id(1))
        .unwrap();
    first.migrate_to_current(2).unwrap();

    let second = Fixture::new();
    second
        .migrator
        .migrate(&second.store, 2, &second.backup, id(99))
        .unwrap();
    assert_eq!(
        second
            .migrator
            .migrate(&second.store, 3, &first.backup, id(100)),
        Err(StorageError::BackupConflict)
    );
    assert_eq!(
        second.migrator.inspect(&second.store).migration_state,
        MigrationState::Required {
            target_version: CURRENT_SCHEMA_VERSION
        }
    );
}

#[test]
fn structurally_invalid_backup_is_rejected_before_restore() {
    let fixture = Fixture::new();
    fixture
        .migrator
        .migrate(&fixture.store, 2, &fixture.backup, id(1))
        .unwrap();
    fixture.migrate_to_current(2).unwrap();
    Connection::open(&fixture.backup)
        .unwrap()
        .execute("DROP TABLE pod0_backup_evidence", [])
        .unwrap();

    assert!(matches!(
        verify_backup(&fixture.backup),
        Err(StorageError::CorruptSchema { .. })
    ));
    let destination = fixture._directory.path().join("invalid-restore.sqlite");
    assert!(restore_backup_to_new_store(&fixture.backup, &destination).is_err());
    assert!(!destination.exists());
}

#[test]
fn cross_language_schema_fixture_matches_rust_contract() {
    let fixture = include_str!("../../../../Fixtures/CoreSchema/schema-status-v1.properties");
    let values: BTreeMap<_, _> = fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once('=').unwrap())
        .collect();

    assert_eq!(values["fixture_version"], "1");
    assert_eq!(values["schema_component"], "kernel");
    assert_eq!(values["stored_version"], "2");
    assert_eq!(values["supported_min"], "0");
    assert_eq!(
        values["supported_max"].parse::<u32>().unwrap(),
        CURRENT_SCHEMA_VERSION
    );
    assert_eq!(values["migration_state"], "required");
    assert_eq!(values["access_mode"], "migration_only");
    assert_eq!(values["target_version"], "5");
    assert_eq!(values["store_id_high"], "10");
    assert_eq!(values["store_id_low"], "11");
    assert_eq!(values["command_id_high"], "1");
    assert_eq!(values["command_id_low"], "2");
    assert_eq!(values["state_revision"], "42");
    assert_eq!(values["operation_stage"], "failed");
    assert_eq!(values["error_kind"], "unsupported");
    assert_eq!(values["error_wire_code"], "9001");
    assert_eq!(values["optional_safe_detail"], "null");
}
