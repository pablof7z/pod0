use pod0_domain::{CommandId, ContentDigest, StateRevision};
use rusqlite::OptionalExtension;

use crate::StorageError;

pub(super) fn fingerprint(value: &str) -> Result<ContentDigest, StorageError> {
    if value.len() != 64 {
        return Err(StorageError::CommandConflict);
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| StorageError::CommandConflict)?;
    }
    Ok(ContentDigest::from_bytes(bytes))
}

pub(super) fn legacy_library_receipt(
    connection: &rusqlite::Connection,
    command_id: CommandId,
    requested: &str,
    operation: &'static str,
) -> Result<Option<StateRevision>, StorageError> {
    let row = connection
        .query_row(
            "SELECT command_fingerprint,applied_revision FROM pod0_library_commands \
             WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite(operation, error))?;
    match row {
        Some((stored, value)) if stored == requested => Ok(Some(revision(value)?)),
        Some(_) => Err(StorageError::CommandConflict),
        None => Ok(None),
    }
}

pub(super) fn next_core_revision(
    connection: &rusqlite::Connection,
    operation: &'static str,
) -> Result<StateRevision, StorageError> {
    let core: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite(operation, error))?;
    revision(core.checked_add(1).ok_or(StorageError::InvalidActivity)?)
}

pub(super) fn revision(value: i64) -> Result<StateRevision, StorageError> {
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
