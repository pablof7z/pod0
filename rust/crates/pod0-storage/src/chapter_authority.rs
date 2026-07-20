use rusqlite::Connection;

use crate::StorageError;

pub fn chapter_store_is_authoritative(path: &std::path::Path) -> Result<bool, StorageError> {
    let connection = crate::chapter_import_store_read::open_current(path)?;
    chapter_is_authoritative(&connection)
}

pub(crate) fn chapter_is_authoritative(connection: &Connection) -> Result<bool, StorageError> {
    let state: (bool, Option<Vec<u8>>) = connection
        .query_row(
            "SELECT authority_active,authority_import_id FROM pod0_chapter_state \
             WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| StorageError::sqlite("read chapter authority", error))?;
    match state {
        (false, None) => Ok(false),
        (true, Some(import_id)) if import_id.len() == 16 => Ok(true),
        _ => Err(StorageError::CorruptSchema {
            detail: "chapter authority state is malformed",
        }),
    }
}

pub(crate) fn require_chapter_authoritative(connection: &Connection) -> Result<(), StorageError> {
    if chapter_is_authoritative(connection)? {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}
