use std::path::{Path, PathBuf};

use pod0_domain::PublicationRecord;
use rusqlite::Connection;

use crate::migration_db::{
    open_connection, user_version, validate_current_database_identity, validate_open_database,
};
use crate::{CURRENT_SCHEMA_VERSION, StorageError};

#[derive(Clone, Debug)]
pub struct PublicationStore {
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicationPrepareOutcome {
    Applied(PublicationRecord),
    Duplicate(PublicationRecord),
}

impl PublicationPrepareOutcome {
    #[must_use]
    pub fn record(&self) -> &PublicationRecord {
        match self {
            Self::Applied(record) | Self::Duplicate(record) => record,
        }
    }
}

impl PublicationStore {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = open_current(path, true)?;
        validate_open_database(&connection, CURRENT_SCHEMA_VERSION)?;
        drop(connection);
        Ok(Self {
            path: path.to_owned(),
        })
    }

    pub(crate) fn read<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let connection = open_current(&self.path, true)?;
        operation(&connection)
    }
}

fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_current_database_identity(&connection, version)?;
    Ok(connection)
}
