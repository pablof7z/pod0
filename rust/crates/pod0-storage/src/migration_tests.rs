use std::sync::atomic::{AtomicBool, Ordering};

use pod0_domain::CommandId;
use rusqlite::Connection;
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
    assert_eq!(
        report.applied_versions,
        (1..=CURRENT_SCHEMA_VERSION).collect::<Vec<_>>()
    );
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
fn subscription_transcript_policy_migration_preserves_existing_rows_as_automatic() {
    let fixture = Fixture::new();
    fixture.migrate_to(28, 1).unwrap();
    Connection::open(&fixture.store)
        .unwrap()
        .execute_batch(
            "PRAGMA foreign_keys=ON;
             INSERT INTO pod0_listening_imports(
                 import_id,source_kind,source_hash,source_generation,podcast_count,
                 subscription_count,episode_count,backup_byte_count,target_revision,state,
                 verified_at_ms
             ) VALUES(
                 x'01010101010101010101010101010101',1,
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 1,1,1,0,1,1,'verified',1
             );
             INSERT INTO pod0_podcasts(
                 podcast_id,kind_code,kind_wire_code,feed_url,feed_key_v1,title,author,
                 image_url,description,language,categories_json,discovered_at_ms,
                 title_is_placeholder,last_refreshed_at_ms,etag,last_modified,
                 source_import_id,library_visible
             ) VALUES(
                 x'02020202020202020202020202020202',1,NULL,NULL,NULL,'Fixture','',
                 NULL,'',NULL,'[]',1,0,NULL,NULL,NULL,
                 x'01010101010101010101010101010101',1
             );
             INSERT INTO pod0_subscriptions(
                 podcast_id,subscribed_at_ms,auto_download_code,auto_download_wire_code,
                 auto_download_latest_count,wifi_only,notifications_enabled,
                 default_playback_rate_permille,source_import_id
             ) VALUES(
                 x'02020202020202020202020202020202',1,3,NULL,NULL,1,1,NULL,
                 x'01010101010101010101010101010101'
             );",
        )
        .unwrap();

    let report = fixture.migrate_to(29, 2).unwrap();
    assert_eq!(report.applied_versions, [29]);
    let connection = Connection::open(&fixture.store).unwrap();
    let stored: (i64, Option<i64>) = connection
        .query_row(
            "SELECT transcript_start_policy_code,transcript_start_policy_wire_code
             FROM pod0_subscriptions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, (1, None));
}

#[test]
fn evidence_schema_upgrade_is_one_step_and_cannot_be_downgraded() {
    let fixture = Fixture::new();
    fixture.migrate_to(6, 1).unwrap();
    let report = fixture.migrate_to(7, 2).unwrap();
    assert_eq!(report.applied_versions, [7]);
    assert_eq!(report.backup.unwrap().schema_version, 6);
    assert_eq!(
        fixture.migrate_to(6, 3),
        Err(StorageError::DowngradeForbidden {
            stored: 7,
            requested: 6,
        })
    );
}

#[test]
fn transcript_artifact_schema_upgrade_is_one_step_and_preserves_v9_backup() {
    let fixture = Fixture::new();
    fixture.migrate_to(9, 1).unwrap();

    let report = fixture.migrate_to(10, 2).unwrap();

    assert_eq!(report.applied_versions, [10]);
    assert_eq!(report.backup.unwrap().schema_version, 9);
    let connection = rusqlite::Connection::open(&fixture.store).unwrap();
    let state: (i64, i64, Option<Vec<u8>>) = connection
        .query_row(
            "SELECT singleton,collection_revision,source_import_id \
             FROM pod0_transcript_state",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(state, (1, 0, None));
}

#[test]
fn retained_library_artifact_upgrade_is_one_step_and_preserves_v10_backup() {
    let fixture = Fixture::new();
    fixture.migrate_to(10, 1).unwrap();

    let report = fixture.migrate_to(11, 2).unwrap();

    assert_eq!(report.applied_versions, [11]);
    assert_eq!(report.backup.unwrap().schema_version, 10);
    let connection = rusqlite::Connection::open(&fixture.store).unwrap();
    let visible: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('pod0_podcasts') \
             WHERE name='library_visible'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(visible, 1);
}

#[test]
fn download_workflow_upgrade_is_one_step_and_preserves_v18_backup() {
    let fixture = Fixture::new();
    fixture.migrate_to(18, 1).unwrap();
    let connection = rusqlite::Connection::open(&fixture.store).unwrap();
    connection
        .execute(
            "INSERT INTO pod0_schema_versions(component,version,updated_at_ms) \
             VALUES('pre-download-sentinel',7,1)",
            [],
        )
        .unwrap();
    drop(connection);

    let report = fixture.migrate_to(19, 2).unwrap();

    assert_eq!(report.applied_versions, [19]);
    assert_eq!(report.backup.unwrap().schema_version, 18);
    let connection = rusqlite::Connection::open(&fixture.store).unwrap();
    let environment: (i64, Option<i64>, Option<i64>, i64) = connection
        .query_row(
            "SELECT network_code,network_wire_code,available_capacity_bytes,observed_at_ms \
             FROM pod0_download_environment WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(environment, (1, None, None, 0));
    let sentinel: i64 = connection
        .query_row(
            "SELECT version FROM pod0_schema_versions WHERE component='pre-download-sentinel'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(sentinel, 7);
}

#[test]
fn transcript_workflow_upgrade_is_one_step_and_rejects_downgrade() {
    let fixture = Fixture::new();
    fixture.migrate_to(19, 1).unwrap();

    let report = fixture.migrate_to(20, 2).unwrap();

    assert_eq!(report.applied_versions, [20]);
    assert_eq!(report.backup.unwrap().schema_version, 19);
    let connection = rusqlite::Connection::open(&fixture.store).unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN(
             'pod0_transcript_workflows','pod0_transcript_attempts',
             'pod0_transcript_evidence_requests','pod0_transcript_workflow_imports')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 4);
    assert_eq!(
        fixture.migrate_to(19, 3),
        Err(StorageError::DowngradeForbidden {
            stored: 20,
            requested: 19,
        })
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
