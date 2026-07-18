use pod0_domain::CommandId;
use rusqlite::Connection;
use tempfile::TempDir;

use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, MigrationClock, MigrationReport, StorageError,
};

pub(crate) struct FixedClock;
impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_322_000_000
    }
}

pub(crate) struct Fixture {
    pub(crate) _directory: TempDir,
    pub(crate) store: std::path::PathBuf,
    pub(crate) backup: std::path::PathBuf,
    pub(crate) migrator: CoreStoreMigrator<FixedClock>,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        Self {
            store: directory.path().join("core.sqlite"),
            backup: directory.path().join("core.backup.sqlite"),
            _directory: directory,
            migrator: CoreStoreMigrator::new(FixedClock),
        }
    }

    pub(crate) fn migrate_to_current(&self, value: u64) -> Result<MigrationReport, StorageError> {
        self.migrator
            .migrate(&self.store, CURRENT_SCHEMA_VERSION, &self.backup, id(value))
    }
}

pub(crate) fn id(value: u64) -> CommandId {
    CommandId::from_parts(0, value)
}

pub(crate) fn table_exists(path: &std::path::Path, table: &str) -> bool {
    Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )
        .unwrap()
}
