use std::path::Path;

use pod0_domain::CommandId;
use rusqlite::{Connection, TransactionBehavior};

use crate::backup::{create_or_reuse_backup, verify_connection};
use crate::migration_db::{
    active_migration, application_id, complete_journal, configure, enable_write_ahead_logging,
    ensure_parent, fail_journal, failed_migration_status, open_connection,
    reconcile_committed_journal, record_backup, start_journal, unfinished_migration, user_version,
    validate_open_database,
};
use crate::model::{
    APPLICATION_ID, AccessMode, BlockedReason, CURRENT_SCHEMA_VERSION,
    MIN_SUPPORTED_SCHEMA_VERSION, MigrationReport, MigrationState, SchemaStatus, StorageError,
};
use crate::schema::{apply_step, validate_schema};

pub trait MigrationClock {
    fn now_milliseconds(&self) -> i64;
}

pub struct CoreStoreMigrator<C> {
    clock: C,
}

impl<C: MigrationClock> CoreStoreMigrator<C> {
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }

    pub fn inspect(&self, path: &Path) -> SchemaStatus {
        if !path.exists() {
            return status_for_version(0, None);
        }
        let Ok(connection) = open_connection(path, true) else {
            return SchemaStatus::blocked(None, BlockedReason::Corrupt);
        };
        let Ok(version) = user_version(&connection) else {
            return SchemaStatus::blocked(None, BlockedReason::Corrupt);
        };
        if verify_connection(&connection).is_err() {
            return SchemaStatus::blocked(Some(version), BlockedReason::Corrupt);
        }
        if version > CURRENT_SCHEMA_VERSION {
            return SchemaStatus::blocked(Some(version), BlockedReason::NewerSchema);
        }
        let Ok(application_id) = application_id(&connection) else {
            return SchemaStatus::blocked(Some(version), BlockedReason::Corrupt);
        };
        if application_id != 0 && application_id != APPLICATION_ID {
            return SchemaStatus::blocked(Some(version), BlockedReason::ForeignDatabase);
        }
        if validate_schema(&connection, version).is_err() {
            return SchemaStatus::blocked(Some(version), BlockedReason::Corrupt);
        }
        if version >= 2 {
            if let Ok(Some((from, to))) = unfinished_migration(&connection, "running") {
                return SchemaStatus {
                    stored_version: Some(version),
                    supported_min: MIN_SUPPORTED_SCHEMA_VERSION,
                    supported_max: CURRENT_SCHEMA_VERSION,
                    access_mode: AccessMode::MigrationOnly,
                    migration_state: MigrationState::InProgress {
                        from_version: from,
                        target_version: to,
                    },
                };
            }
            if unfinished_migration(&connection, "failed").is_ok_and(|value| value.is_some()) {
                return SchemaStatus::blocked(Some(version), BlockedReason::FailedMigration);
            }
        }
        status_for_version(version, Some(version))
    }

    pub fn migrate(
        &self,
        path: &Path,
        target_version: u32,
        backup_path: &Path,
        migration_id: CommandId,
    ) -> Result<MigrationReport, StorageError> {
        self.migrate_with_observer(
            path,
            target_version,
            backup_path,
            migration_id,
            &NoopObserver,
        )
    }

    pub(crate) fn migrate_with_observer<O: MigrationObserver>(
        &self,
        path: &Path,
        target_version: u32,
        backup_path: &Path,
        requested_migration_id: CommandId,
        observer: &O,
    ) -> Result<MigrationReport, StorageError> {
        validate_target(target_version)?;
        ensure_parent(path)?;
        let mut connection = open_connection(path, false)?;
        configure(&connection)?;
        verify_connection(&connection)?;
        let from_version = user_version(&connection)?;
        validate_open_database(&connection, from_version)?;
        if target_version < from_version {
            return Err(StorageError::DowngradeForbidden {
                stored: from_version,
                requested: target_version,
            });
        }

        let mut resumed = false;
        let mut migration_id = requested_migration_id;
        if from_version >= 2 {
            resumed = reconcile_committed_journal(&connection, self.clock.now_milliseconds())? > 0;
            if let Some(active) = active_migration(&connection)? {
                if active.target_version > target_version {
                    return Err(StorageError::FailedMigration {
                        from: active.from_version,
                        to: active.target_version,
                    });
                }
                migration_id = active.migration_id;
                resumed = true;
            }
            let (blocking_failure, retrying_legacy_backup_conflict) =
                failed_migration_status(&connection, migration_id)?;
            if let Some((from, to)) = blocking_failure {
                return Err(StorageError::FailedMigration { from, to });
            }
            resumed |= retrying_legacy_backup_conflict;
        }

        if target_version == from_version {
            return Ok(MigrationReport {
                migration_id,
                from_version,
                to_version: from_version,
                applied_versions: Vec::new(),
                resumed_from_journal: resumed,
                backup: None,
            });
        }

        let backup = if from_version == 0 {
            None
        } else {
            Some(create_or_reuse_backup(
                &connection,
                backup_path,
                from_version,
            )?)
        };
        enable_write_ahead_logging(&connection)?;
        let mut applied_versions = Vec::new();
        for version in (from_version + 1)..=target_version {
            let step_from = version - 1;
            if step_from >= 2 {
                start_journal(
                    &connection,
                    migration_id,
                    step_from,
                    version,
                    self.clock.now_milliseconds(),
                )?;
                observer.reached(MigrationBoundary::JournalPersisted { target: version })?;
            }
            if let Err(error) = run_step(
                &mut connection,
                step_from,
                version,
                self.clock.now_milliseconds(),
                migration_id,
                backup.as_ref(),
                observer,
            ) {
                if step_from >= 2 && error != StorageError::Interrupted {
                    fail_journal(
                        &connection,
                        migration_id,
                        version,
                        error.code(),
                        self.clock.now_milliseconds(),
                    )?;
                }
                return Err(error);
            }
            applied_versions.push(version);
            observer.reached(MigrationBoundary::AfterStepCommit { target: version })?;
        }
        validate_schema(&connection, target_version)?;
        Ok(MigrationReport {
            migration_id,
            from_version,
            to_version: target_version,
            applied_versions,
            resumed_from_journal: resumed,
            backup,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MigrationBoundary {
    JournalPersisted { target: u32 },
    BeforeStepCommit { target: u32 },
    AfterStepCommit { target: u32 },
}

pub(crate) trait MigrationObserver {
    fn reached(&self, boundary: MigrationBoundary) -> Result<(), StorageError>;
}

struct NoopObserver;

impl MigrationObserver for NoopObserver {
    fn reached(&self, _: MigrationBoundary) -> Result<(), StorageError> {
        Ok(())
    }
}

fn status_for_version(version: u32, stored: Option<u32>) -> SchemaStatus {
    let (access_mode, migration_state) = if version == 0 {
        (AccessMode::MigrationOnly, MigrationState::Fresh)
    } else if version < CURRENT_SCHEMA_VERSION {
        (
            AccessMode::MigrationOnly,
            MigrationState::Required {
                target_version: CURRENT_SCHEMA_VERSION,
            },
        )
    } else {
        (AccessMode::ReadWrite, MigrationState::Ready)
    };
    SchemaStatus {
        stored_version: stored,
        supported_min: MIN_SUPPORTED_SCHEMA_VERSION,
        supported_max: CURRENT_SCHEMA_VERSION,
        access_mode,
        migration_state,
    }
}

fn validate_target(target: u32) -> Result<(), StorageError> {
    if (1..=CURRENT_SCHEMA_VERSION).contains(&target) {
        Ok(())
    } else {
        Err(StorageError::UnsupportedTarget {
            requested: target,
            supported: CURRENT_SCHEMA_VERSION,
        })
    }
}

fn run_step<O: MigrationObserver>(
    connection: &mut Connection,
    from_version: u32,
    version: u32,
    observed_at_ms: i64,
    migration_id: CommandId,
    backup: Option<&crate::model::BackupEvidence>,
    observer: &O,
) -> Result<(), StorageError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin schema migration", error))?;
    apply_step(&transaction, version, observed_at_ms, migration_id)?;
    if version >= 2 {
        if let Some(evidence) = backup {
            record_backup(&transaction, migration_id, evidence, observed_at_ms)?;
        }
        complete_journal(
            &transaction,
            migration_id,
            from_version,
            version,
            observed_at_ms,
        )?;
    }
    observer.reached(MigrationBoundary::BeforeStepCommit { target: version })?;
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit schema migration", error))
}
