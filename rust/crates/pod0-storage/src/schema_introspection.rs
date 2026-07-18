use std::collections::BTreeSet;

use rusqlite::Connection;

use crate::StorageError;

pub(crate) fn require_columns(
    connection: &Connection,
    table: &str,
    expected: &[&str],
) -> Result<(), StorageError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| StorageError::sqlite("inspect table", error))?;
    let actual = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| StorageError::sqlite("inspect table columns", error))?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| StorageError::sqlite("read table columns", error))?;
    let expected = expected.iter().map(ToString::to_string).collect();
    if actual == expected {
        Ok(())
    } else {
        Err(StorageError::CorruptSchema {
            detail: "table columns do not match schema",
        })
    }
}

pub(crate) fn table_names(connection: &Connection) -> Result<BTreeSet<String>, StorageError> {
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .map_err(|error| StorageError::sqlite("inspect database tables", error))?;
    statement
        .query_map([], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("query database tables", error))?
        .collect::<Result<_, _>>()
        .map_err(|error| StorageError::sqlite("read database tables", error))
}
