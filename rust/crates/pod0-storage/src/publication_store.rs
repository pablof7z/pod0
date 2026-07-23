use std::path::{Path, PathBuf};

use pod0_domain::PublicationRecord;
use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::migration_db::{configure, open_connection, user_version, validate_open_database};
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

    pub(crate) fn write<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = open_current(&self.path, false)?;
        configure(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin publication mutation", error))?;
        let result = operation(&transaction)?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit publication mutation", error))?;
        Ok(result)
    }
}

fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "publication store schema is not current",
        });
    }
    Ok(connection)
}
