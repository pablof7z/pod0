use pod0_domain::{ScheduledOccurrenceId, ScheduledTaskId, StateRevision};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::{ScheduledAgentCommandContext, StorageError};

type CommandReceiptDatabaseRow = (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, i64);

#[derive(Clone, Copy)]
pub(crate) struct CommandReceipt {
    pub(crate) task_id: Option<ScheduledTaskId>,
    pub(crate) occurrence_id: Option<ScheduledOccurrenceId>,
    pub(crate) applied_revision: StateRevision,
}

pub(crate) fn command_receipt(
    transaction: &Transaction<'_>,
    context: &ScheduledAgentCommandContext,
) -> Result<Option<CommandReceipt>, StorageError> {
    let row: Option<CommandReceiptDatabaseRow> = transaction
        .query_row(
            "SELECT command_fingerprint,task_id,occurrence_id,applied_revision \
             FROM pod0_scheduled_command_receipts WHERE command_id=?1",
            [context.command_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read scheduled command receipt", error))?;
    let Some((fingerprint, task, occurrence, revision)) = row else {
        return Ok(None);
    };
    if fingerprint.as_slice() != context.command_fingerprint {
        return Err(StorageError::ScheduledAgentCommandConflict);
    }
    Ok(Some(CommandReceipt {
        task_id: task
            .as_deref()
            .map(crate::scheduled_agent_store_codec::task_id)
            .transpose()?,
        occurrence_id: occurrence
            .as_deref()
            .map(crate::scheduled_agent_store_codec::occurrence_id)
            .transpose()?,
        applied_revision: crate::scheduled_agent_store_codec::revision(revision)?,
    }))
}

pub(crate) fn finish_command(
    transaction: &Transaction<'_>,
    context: &ScheduledAgentCommandContext,
    task_id: Option<ScheduledTaskId>,
    occurrence_id: Option<ScheduledOccurrenceId>,
) -> Result<StateRevision, StorageError> {
    let next = advance_core_revision(transaction)?;
    transaction
        .execute(
            "INSERT INTO pod0_scheduled_command_receipts(command_id,command_fingerprint,task_id,\
         occurrence_id,applied_revision,completed_at_ms) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                context.command_id.into_bytes().as_slice(),
                context.command_fingerprint.as_slice(),
                task_id.map(|value| value.into_bytes().to_vec()),
                occurrence_id.map(|value| value.into_bytes().to_vec()),
                i64::try_from(next.value)
                    .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?,
                context.observed_at.value(),
            ],
        )
        .map_err(|error| StorageError::sqlite("record scheduled command receipt", error))?;
    Ok(next)
}

pub(crate) fn advance_core_revision(
    transaction: &Transaction<'_>,
) -> Result<StateRevision, StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT core_revision FROM pod0_scheduled_agent_authority WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read scheduled core revision", error))?;
    let next = current.checked_add(1).ok_or(StorageError::CorruptSchema {
        detail: "scheduled core revision exhausted",
    })?;
    transaction
        .execute(
            "UPDATE pod0_scheduled_agent_authority SET core_revision=?1 WHERE singleton=1",
            [next],
        )
        .map_err(|error| StorageError::sqlite("advance scheduled core revision", error))?;
    crate::scheduled_agent_store_codec::revision(next)
}

pub(crate) fn core_revision(transaction: &Transaction<'_>) -> Result<StateRevision, StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT core_revision FROM pod0_scheduled_agent_authority WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read scheduled core revision", error))?;
    crate::scheduled_agent_store_codec::revision(current)
}

pub(crate) fn validate_context(context: &ScheduledAgentCommandContext) -> Result<(), StorageError> {
    if context.observed_at.value() < 0 {
        Err(StorageError::ScheduledAgentWorkflowConflict)
    } else {
        Ok(())
    }
}

pub(crate) fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ScheduledAgentWorkflowConflict)
}
