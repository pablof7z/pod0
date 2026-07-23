use pod0_domain::{CommandId, MemoryId, MemoryRevision, StateRevision};
use rusqlite::{OptionalExtension, Transaction};

use crate::library_store::finish_command;
use crate::{StorageError, library_store::command_was_applied};

pub(crate) fn finish_memory_command(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let revision = finish_command(transaction, command_id, fingerprint, observed_at_ms)?;
    let value = i64::try_from(revision.value).map_err(|_| StorageError::CorruptSchema {
        detail: "memory collection revision is malformed",
    })?;
    transaction
        .execute(
            "UPDATE pod0_memory_state SET collection_revision=?1 WHERE singleton=1",
            [value],
        )
        .map_err(|error| StorageError::sqlite("advance memory collection revision", error))?;
    transaction
        .execute(
            "UPDATE pod0_domain_cutovers SET core_revision=?1 WHERE domain='memories'",
            [value],
        )
        .map_err(|error| StorageError::sqlite("advance memory cutover revision", error))?;
    Ok(revision)
}

pub(crate) fn collection_revision(
    transaction: &Transaction<'_>,
) -> Result<StateRevision, StorageError> {
    let value: i64 = transaction
        .query_row(
            "SELECT collection_revision FROM pod0_memory_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read memory collection revision", error))?;
    Ok(StateRevision::new(u64::try_from(value).map_err(|_| {
        StorageError::CorruptSchema {
            detail: "memory collection revision is malformed",
        }
    })?))
}

pub(crate) fn command_replay(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
) -> Result<Option<StateRevision>, StorageError> {
    command_was_applied(transaction, command_id, fingerprint)
}

pub(crate) fn memory_revision(
    transaction: &Transaction<'_>,
    memory_id: MemoryId,
) -> Result<MemoryRevision, StorageError> {
    let value: Option<i64> = transaction
        .query_row(
            "SELECT memory_revision FROM pod0_memories WHERE memory_id=?1",
            [memory_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read memory revision", error))?;
    let value = value.ok_or(StorageError::EntityNotFound)?;
    Ok(MemoryRevision::new(u64::try_from(value).map_err(|_| {
        StorageError::CorruptSchema {
            detail: "memory revision is malformed",
        }
    })?))
}

pub(crate) fn invalidate_compiled_memory(
    transaction: &Transaction<'_>,
) -> Result<(), StorageError> {
    transaction
        .execute("DELETE FROM pod0_compiled_memory WHERE singleton=1", [])
        .map_err(|error| StorageError::sqlite("invalidate compiled memory", error))?;
    Ok(())
}
