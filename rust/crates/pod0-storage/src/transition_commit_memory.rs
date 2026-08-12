use pod0_application::{
    RequestDisposition, RequestRejectionReason, UserArtifactActivityInput, UserArtifactMutation,
    UserArtifactTransition, plan_user_artifact_activity,
};
use pod0_domain::{
    CommandId, MemoryId, MemoryRevision, MemorySource, StateRevision, UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use super::application_support::fingerprint;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

#[path = "transition_commit_memory_preflight.rs"]
mod preflight;
use preflight::{memory_collection_revision, preflight};

pub(super) enum MemoryWrite<'a> {
    Create {
        content: &'a str,
        source: MemorySource,
    },
    Update {
        memory_id: MemoryId,
        expected: MemoryRevision,
        content: &'a str,
    },
    SetDeleted {
        memory_id: MemoryId,
        expected: MemoryRevision,
        deleted: bool,
    },
    Clear {
        expected: StateRevision,
    },
}

pub(crate) fn commit_memory_create(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    content: &str,
    source: MemorySource,
    observed_at_ms: i64,
) -> Result<(StateRevision, MemoryId, MemoryRevision), StorageError> {
    let memory_id = MemoryId::from_bytes(command_id.into_bytes());
    let collection = commit_memory_write(
        path,
        command_id,
        command_fingerprint,
        MemoryWrite::Create { content, source },
        observed_at_ms,
    )?;
    let entity = memory_revision(path, memory_id)?;
    Ok((collection, memory_id, entity))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_memory_update(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    memory_id: MemoryId,
    expected: MemoryRevision,
    content: &str,
    observed_at_ms: i64,
) -> Result<(StateRevision, MemoryRevision), StorageError> {
    let collection = commit_memory_write(
        path,
        command_id,
        command_fingerprint,
        MemoryWrite::Update {
            memory_id,
            expected,
            content,
        },
        observed_at_ms,
    )?;
    Ok((collection, memory_revision(path, memory_id)?))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_memory_deleted(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    memory_id: MemoryId,
    expected: MemoryRevision,
    deleted: bool,
    observed_at_ms: i64,
) -> Result<(StateRevision, MemoryRevision), StorageError> {
    let collection = commit_memory_write(
        path,
        command_id,
        command_fingerprint,
        MemoryWrite::SetDeleted {
            memory_id,
            expected,
            deleted,
        },
        observed_at_ms,
    )?;
    Ok((collection, memory_revision(path, memory_id)?))
}

pub(crate) fn commit_memory_clear(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    expected: StateRevision,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit_memory_write(
        path,
        command_id,
        command_fingerprint,
        MemoryWrite::Clear { expected },
        observed_at_ms,
    )
}

fn commit_memory_write(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    write: MemoryWrite<'_>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let ingress = TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint: fingerprint(command_fingerprint)?,
        };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let (current, committed, legacy, subject, rejection) =
                preflight(transaction, command_id, command_fingerprint, &write)?;
            let disposition = if legacy.is_some() {
                RequestDisposition::Duplicate
            } else if let Some(reason) = rejection {
                RequestDisposition::Rejected { reason }
            } else {
                RequestDisposition::Accepted
            };
            plan_user_artifact_activity(UserArtifactActivityInput {
                command_id,
                subject,
                episode_ids: Vec::new(),
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
                transition: UserArtifactTransition::MemoryChanged,
                disposition,
            })
            .map(|plan| {
                plan.map_mutation(|mutation| {
                    (mutation, committed, legacy.unwrap_or(current))
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            (UserArtifactMutation::Apply, committed, _) => {
                require_revision(transaction, expected)?;
                let actual = apply(
                    transaction,
                    command_id,
                    command_fingerprint,
                    &write,
                    observed_at_ms,
                )?;
                if actual != committed {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(actual)
            }
            (UserArtifactMutation::None, _, return_revision) => {
                require_revision(transaction, expected)?;
                Ok(return_revision)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            Ok(receipt.committed_revision)
        }
        RequestDisposition::Rejected { reason } => Err(rejection_error(reason)),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn apply(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    write: &MemoryWrite<'_>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    match write {
        MemoryWrite::Create { content, source } => {
            crate::library_store_memory_write::create_memory_in_transaction(
                transaction,
                command_id,
                MemoryId::from_bytes(command_id.into_bytes()),
                fingerprint,
                content,
                *source,
                observed_at_ms,
            )
            .map(|value| value.0)
        }
        MemoryWrite::Update {
            memory_id,
            expected,
            content,
        } => crate::library_store_memory_write::update_memory_in_transaction(
            transaction,
            command_id,
            fingerprint,
            *memory_id,
            *expected,
            content,
            observed_at_ms,
        )
        .map(|value| value.0),
        MemoryWrite::SetDeleted {
            memory_id,
            expected,
            deleted,
        } => crate::library_store_memory_write::set_memory_deleted_in_transaction(
            transaction,
            command_id,
            fingerprint,
            *memory_id,
            *expected,
            *deleted,
            observed_at_ms,
        )
        .map(|value| value.0),
        MemoryWrite::Clear { expected } => {
            crate::library_store_memory_write::clear_memories_in_transaction(
                transaction,
                command_id,
                fingerprint,
                *expected,
                observed_at_ms,
            )
        }
    }
}

fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    if memory_collection_revision(transaction)? == expected {
        Ok(())
    } else {
        Err(StorageError::RevisionConflict)
    }
}

fn memory_revision(
    path: &std::path::Path,
    memory_id: MemoryId,
) -> Result<MemoryRevision, StorageError> {
    crate::LibraryStore::open_authoritative(path)?
        .read(|connection| crate::memory_store_support::memory_revision(connection, memory_id))
}

fn rejection_error(reason: RequestRejectionReason) -> StorageError {
    match reason {
        RequestRejectionReason::MissingSubject => StorageError::EntityNotFound,
        RequestRejectionReason::RevisionConflict => StorageError::RevisionConflict,
        _ => StorageError::InvalidMemory,
    }
}
