use pod0_application::{ActivitySubject, RequestRejectionReason};
use pod0_domain::{CommandId, MemoryId, MemoryRevision, MemorySource, StateRevision};

use super::MemoryWrite;
use crate::StorageError;
use crate::transition_commit::application_support::{
    legacy_library_receipt, next_core_revision, revision,
};

type Preflight = (
    StateRevision,
    StateRevision,
    Option<StateRevision>,
    ActivitySubject,
    Option<RequestRejectionReason>,
);

pub(super) fn preflight(
    connection: &rusqlite::Connection,
    command_id: CommandId,
    command_fingerprint: &str,
    write: &MemoryWrite<'_>,
) -> Result<Preflight, StorageError> {
    crate::memory_store_read::require_memories_authoritative(connection)?;
    let current = memory_collection_revision(connection)?;
    let committed = next_core_revision(connection, "read memory core revision")?;
    let legacy = legacy_library_receipt(
        connection,
        command_id,
        command_fingerprint,
        "read memory command receipt",
    )?;
    let (subject, rejection) = match write {
        MemoryWrite::Create { content, source } => {
            let memory_id = MemoryId::from_bytes(command_id.into_bytes());
            let invalid = pod0_domain::validate_new_memory(content, *source).is_err()
                || memory_exists(connection, memory_id)?;
            (
                ActivitySubject::Memory { memory_id },
                invalid.then_some(RequestRejectionReason::Invalid),
            )
        }
        MemoryWrite::Update {
            memory_id,
            expected,
            content,
        } => (
            ActivitySubject::Memory {
                memory_id: *memory_id,
            },
            existing_rejection(connection, *memory_id, *expected)?.or_else(|| {
                pod0_domain::validate_new_memory(content, MemorySource::Agent)
                    .is_err()
                    .then_some(RequestRejectionReason::Invalid)
            }),
        ),
        MemoryWrite::SetDeleted {
            memory_id,
            expected,
            ..
        } => (
            ActivitySubject::Memory {
                memory_id: *memory_id,
            },
            existing_rejection(connection, *memory_id, *expected)?,
        ),
        MemoryWrite::Clear { expected } => (
            ActivitySubject::Global,
            (*expected != current).then_some(RequestRejectionReason::RevisionConflict),
        ),
    };
    Ok((current, committed, legacy, subject, rejection))
}

fn memory_exists(
    connection: &rusqlite::Connection,
    memory_id: MemoryId,
) -> Result<bool, StorageError> {
    match crate::memory_store_support::memory_revision(connection, memory_id) {
        Ok(_) => Ok(true),
        Err(StorageError::EntityNotFound) => Ok(false),
        Err(error) => Err(error),
    }
}

fn existing_rejection(
    connection: &rusqlite::Connection,
    memory_id: MemoryId,
    expected: MemoryRevision,
) -> Result<Option<RequestRejectionReason>, StorageError> {
    match crate::memory_store_support::memory_revision(connection, memory_id) {
        Ok(actual) if actual == expected => Ok(None),
        Ok(_) => Ok(Some(RequestRejectionReason::RevisionConflict)),
        Err(StorageError::EntityNotFound) => Ok(Some(RequestRejectionReason::MissingSubject)),
        Err(error) => Err(error),
    }
}

pub(super) fn memory_collection_revision(
    connection: &rusqlite::Connection,
) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT collection_revision FROM pod0_memory_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read memory transition revision", error))?;
    revision(value)
}
