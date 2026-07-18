use std::sync::atomic::{AtomicBool, Ordering};

use pod0_domain::CommandId;
use tempfile::TempDir;

use crate::migration::{MigrationBoundary, MigrationObserver};
use crate::*;

#[derive(Clone, Copy)]
struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_322_000_000
    }
}

struct InterruptOnce {
    boundary: MigrationBoundary,
    triggered: AtomicBool,
}

impl InterruptOnce {
    fn at(boundary: MigrationBoundary) -> Self {
        Self {
            boundary,
            triggered: AtomicBool::new(false),
        }
    }
}

impl MigrationObserver for InterruptOnce {
    fn reached(&self, boundary: MigrationBoundary) -> Result<(), StorageError> {
        if boundary == self.boundary && !self.triggered.swap(true, Ordering::SeqCst) {
            Err(StorageError::Interrupted)
        } else {
            Ok(())
        }
    }
}

#[test]
fn fresh_store_migrates_transactionally_to_current() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.migrate_to(0, 1),
        Err(StorageError::UnsupportedTarget { requested: 0, .. })
    ));
    assert!(!fixture.store.exists());
    assert_eq!(
        fixture.migrator.inspect(&fixture.store),
        SchemaStatus {
            stored_version: None,
            supported_min: 0,
            supported_max: CURRENT_SCHEMA_VERSION,
            access_mode: AccessMode::MigrationOnly,
            migration_state: MigrationState::Fresh,
        }
    );

    let report = fixture.migrate_to(CURRENT_SCHEMA_VERSION, 1).unwrap();

    assert_eq!(report.from_version, 0);
    assert_eq!(report.applied_versions, vec![1, 2, 3, 4, 5]);
    assert!(report.backup.is_none());
    assert_eq!(
        fixture.migrator.inspect(&fixture.store).migration_state,
        MigrationState::Ready
    );
}

#[test]
fn one_and_multiple_version_upgrades_preserve_verified_backups() {
    let one_behind = Fixture::new();
    one_behind.migrate_to(2, 1).unwrap();
    let report = one_behind.migrate_to(3, 2).unwrap();
    assert_eq!(report.applied_versions, [3]);
    assert_eq!(
        report.backup.as_ref().map(|item| item.schema_version),
        Some(2)
    );

    let multiple_behind = Fixture::new();
    multiple_behind.migrate_to(1, 3).unwrap();
    let report = multiple_behind.migrate_to(3, 4).unwrap();
    assert_eq!(report.applied_versions, [2, 3]);
    assert_eq!(
        report.backup.as_ref().map(|item| item.schema_version),
        Some(1)
    );
}

#[test]
fn interruption_rolls_back_the_step_and_restart_resumes_the_journal() {
    let fixture = Fixture::new();
    fixture.migrate_to(2, 1).unwrap();
    let original_id = command_id(2);
    let observer = InterruptOnce::at(MigrationBoundary::BeforeStepCommit { target: 3 });

    let error = fixture
        .migrator
        .migrate_with_observer(&fixture.store, 3, &fixture.backup, original_id, &observer)
        .unwrap_err();

    assert_eq!(error, StorageError::Interrupted);
    assert_eq!(
        fixture.migrator.inspect(&fixture.store).migration_state,
        MigrationState::InProgress {
            from_version: 2,
            target_version: 3,
        }
    );
    let report = fixture.migrate_to(3, 99).unwrap();
    assert!(report.resumed_from_journal);
    assert_eq!(report.migration_id, original_id);
    assert!(report.backup.is_some_and(|backup| backup.reused_existing));
}

#[test]
fn committed_step_and_backup_evidence_are_atomic_across_restart() {
    let fixture = Fixture::new();
    fixture.migrate_to(2, 1).unwrap();
    let observer = InterruptOnce::at(MigrationBoundary::AfterStepCommit { target: 3 });
    assert_eq!(
        fixture
            .migrator
            .migrate_with_observer(&fixture.store, 3, &fixture.backup, command_id(2), &observer,)
            .unwrap_err(),
        StorageError::Interrupted
    );

    let report = fixture.migrate_to(3, 3).unwrap();
    assert!(!report.resumed_from_journal);
    assert!(report.applied_versions.is_empty());
    assert_eq!(report.to_version, 3);
    assert_eq!(verify_backup(&fixture.backup).unwrap().schema_version, 2);
}

struct Fixture {
    _directory: TempDir,
    store: std::path::PathBuf,
    backup: std::path::PathBuf,
    migrator: CoreStoreMigrator<FixedClock>,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        Self {
            store: directory.path().join("core.sqlite"),
            backup: directory.path().join("core.backup.sqlite"),
            _directory: directory,
            migrator: CoreStoreMigrator::new(FixedClock),
        }
    }

    fn migrate_to(&self, version: u32, id: u64) -> Result<MigrationReport, StorageError> {
        self.migrator
            .migrate(&self.store, version, &self.backup, command_id(id))
    }
}

fn command_id(value: u64) -> CommandId {
    CommandId::from_parts(0, value)
}
