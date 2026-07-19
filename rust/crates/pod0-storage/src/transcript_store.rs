use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::migration_db::{configure, open_connection, user_version, validate_open_database};
use crate::{CURRENT_SCHEMA_VERSION, StorageError};

#[derive(Clone, Debug)]
pub struct TranscriptStore {
    path: PathBuf,
}

impl TranscriptStore {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = open_current(path, true)?;
        require_valid_foreign_keys(&connection)?;
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
            .map_err(|error| StorageError::sqlite("begin transcript command", error))?;
        let output = operation(&transaction)?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit transcript command", error))?;
        Ok(output)
    }
}

fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "transcript store schema is not current",
        });
    }
    Ok(connection)
}

fn require_valid_foreign_keys(connection: &Connection) -> Result<(), StorageError> {
    let violation: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|error| StorageError::sqlite("verify transcript references", error))?;
    if violation.is_none() {
        Ok(())
    } else {
        Err(StorageError::InvalidTranscriptArtifact)
    }
}
