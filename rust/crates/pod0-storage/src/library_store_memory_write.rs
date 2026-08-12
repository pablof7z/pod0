use pod0_domain::{
    CommandId, MemoryId, MemoryRevision, MemorySource, StateRevision, validate_new_memory,
};
use rusqlite::params;

use crate::StorageError;
use crate::memory_store_read::{encode_source, require_memories_authoritative};
use crate::memory_store_support::{
    collection_revision, command_replay, finish_memory_command, invalidate_compiled_memory,
    memory_revision,
};

pub(crate) fn create_memory_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    memory_id: MemoryId,
    command_fingerprint: &str,
    content: &str,
    source: MemorySource,
    observed_at_ms: i64,
) -> Result<(StateRevision, MemoryId, MemoryRevision), StorageError> {
    require_memories_authoritative(transaction)?;
    if let Some(revision) = command_replay(transaction, command_id, command_fingerprint)? {
        return Ok((
            revision,
            memory_id,
            memory_revision(transaction, memory_id)?,
        ));
    }
    validate_new_memory(content, source).map_err(|_| StorageError::InvalidMemory)?;
    transaction
        .execute(
            "INSERT INTO pod0_memories(memory_id,memory_revision,content,source_code,created_at_ms,\
             updated_at_ms,deleted,created_command_id) VALUES(?1,1,?2,?3,?4,?4,0,?5)",
            params![
                memory_id.into_bytes().as_slice(),
                content,
                encode_source(source)?,
                observed_at_ms,
                command_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("create memory", error))?;
    invalidate_compiled_memory(transaction)?;
    let revision =
        finish_memory_command(transaction, command_id, command_fingerprint, observed_at_ms)?;
    Ok((revision, memory_id, MemoryRevision::INITIAL))
}

pub(crate) fn update_memory_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    command_fingerprint: &str,
    memory_id: MemoryId,
    expected_revision: MemoryRevision,
    content: &str,
    observed_at_ms: i64,
) -> Result<(StateRevision, MemoryRevision), StorageError> {
    require_memories_authoritative(transaction)?;
    if let Some(revision) = command_replay(transaction, command_id, command_fingerprint)? {
        return Ok((revision, memory_revision(transaction, memory_id)?));
    }
    validate_new_memory(content, MemorySource::Agent).map_err(|_| StorageError::InvalidMemory)?;
    if memory_revision(transaction, memory_id)? != expected_revision {
        return Err(StorageError::RevisionConflict);
    }
    let expected =
        i64::try_from(expected_revision.value).map_err(|_| StorageError::RevisionConflict)?;
    let changed = transaction.execute(
        "UPDATE pod0_memories SET content=?1,memory_revision=memory_revision+1,updated_at_ms=?2 \
         WHERE memory_id=?3 AND memory_revision=?4",
        params![content, observed_at_ms, memory_id.into_bytes().as_slice(), expected],
    ).map_err(|error| StorageError::sqlite("update memory", error))?;
    finish_memory_mutation(
        transaction,
        command_id,
        command_fingerprint,
        expected_revision,
        changed,
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn set_memory_deleted_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    command_fingerprint: &str,
    memory_id: MemoryId,
    expected_revision: MemoryRevision,
    deleted: bool,
    observed_at_ms: i64,
) -> Result<(StateRevision, MemoryRevision), StorageError> {
    require_memories_authoritative(transaction)?;
    if let Some(revision) = command_replay(transaction, command_id, command_fingerprint)? {
        return Ok((revision, memory_revision(transaction, memory_id)?));
    }
    if memory_revision(transaction, memory_id)? != expected_revision {
        return Err(StorageError::RevisionConflict);
    }
    let expected =
        i64::try_from(expected_revision.value).map_err(|_| StorageError::RevisionConflict)?;
    let changed = transaction.execute(
        "UPDATE pod0_memories SET deleted=?1,memory_revision=memory_revision+1,updated_at_ms=?2 \
         WHERE memory_id=?3 AND memory_revision=?4",
        params![i64::from(deleted), observed_at_ms, memory_id.into_bytes().as_slice(), expected],
    ).map_err(|error| StorageError::sqlite("update memory deletion", error))?;
    finish_memory_mutation(
        transaction,
        command_id,
        command_fingerprint,
        expected_revision,
        changed,
        observed_at_ms,
    )
}

fn finish_memory_mutation(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    expected: MemoryRevision,
    changed: usize,
    observed_at_ms: i64,
) -> Result<(StateRevision, MemoryRevision), StorageError> {
    if changed != 1 {
        return Err(StorageError::RevisionConflict);
    }
    invalidate_compiled_memory(transaction)?;
    let collection = finish_memory_command(transaction, command_id, fingerprint, observed_at_ms)?;
    let next = expected
        .value
        .checked_add(1)
        .ok_or(StorageError::RevisionConflict)?;
    Ok((collection, MemoryRevision::new(next)))
}

pub(crate) fn clear_memories_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    command_fingerprint: &str,
    expected_collection_revision: StateRevision,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    require_memories_authoritative(transaction)?;
    if let Some(revision) = command_replay(transaction, command_id, command_fingerprint)? {
        return Ok(revision);
    }
    if collection_revision(transaction)? != expected_collection_revision {
        return Err(StorageError::RevisionConflict);
    }
    transaction
        .execute(
            "UPDATE pod0_memories SET deleted=1,memory_revision=memory_revision+1,updated_at_ms=?1 \
         WHERE deleted=0",
            [observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("clear memories", error))?;
    invalidate_compiled_memory(transaction)?;
    finish_memory_command(transaction, command_id, command_fingerprint, observed_at_ms)
}
