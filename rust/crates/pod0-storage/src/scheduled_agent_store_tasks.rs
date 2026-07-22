use pod0_application::{ScheduledTaskDefinition, validate_scheduled_task_definition};
use pod0_domain::{ScheduledOccurrenceId, ScheduledTaskId, StateRevision};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::scheduled_agent_store_read::read_task;
use crate::{
    ScheduledAgentCommandContext, ScheduledAgentStore, ScheduledTaskMutationOutcome,
    ScheduledTaskRemovalOutcome, StorageError,
};

type CommandReceiptDatabaseRow = (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, i64);

impl ScheduledAgentStore {
    pub fn ensure_task(
        &self,
        context: ScheduledAgentCommandContext,
        definition: ScheduledTaskDefinition,
    ) -> Result<ScheduledTaskMutationOutcome, StorageError> {
        validate_context(&context)?;
        validate_scheduled_task_definition(&definition)
            .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?;
        if definition.revision != StateRevision::new(1)
            || definition.created_at != context.observed_at
            || definition.last_run_at.is_some()
        {
            return Err(StorageError::ScheduledAgentWorkflowConflict);
        }
        self.write(|transaction| {
            if let Some(receipt) = command_receipt(transaction, &context)? {
                let task_id = receipt
                    .task_id
                    .ok_or(StorageError::ScheduledAgentCommandConflict)?;
                let task = read_task(transaction, task_id, false)?
                    .ok_or(StorageError::ScheduledAgentTaskNotFound)?;
                return Ok(ScheduledTaskMutationOutcome::Duplicate(task));
            }
            if let Some(existing) = read_task(transaction, definition.task_id, false)? {
                if !same_definition(&existing, &definition) {
                    return Err(StorageError::ScheduledAgentWorkflowConflict);
                }
                finish_command(transaction, &context, Some(definition.task_id), None)?;
                return Ok(ScheduledTaskMutationOutcome::Applied(existing));
            }
            insert_task(transaction, &definition, context.observed_at.value())?;
            finish_command(transaction, &context, Some(definition.task_id), None)?;
            Ok(ScheduledTaskMutationOutcome::Applied(definition))
        })
    }

    pub fn update_task(
        &self,
        context: ScheduledAgentCommandContext,
        expected_revision: StateRevision,
        definition: ScheduledTaskDefinition,
    ) -> Result<ScheduledTaskMutationOutcome, StorageError> {
        validate_context(&context)?;
        validate_scheduled_task_definition(&definition)
            .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?;
        if definition.revision.value != expected_revision.value.saturating_add(1) {
            return Err(StorageError::ScheduledAgentWorkflowConflict);
        }
        self.write(|transaction| {
            if let Some(receipt) = command_receipt(transaction, &context)? {
                let task_id = receipt
                    .task_id
                    .ok_or(StorageError::ScheduledAgentCommandConflict)?;
                let task = read_task(transaction, task_id, false)?
                    .ok_or(StorageError::ScheduledAgentTaskNotFound)?;
                return Ok(ScheduledTaskMutationOutcome::Duplicate(task));
            }
            let existing = read_task(transaction, definition.task_id, true)?
                .ok_or(StorageError::ScheduledAgentTaskNotFound)?;
            if existing.revision != expected_revision
                || definition.created_at != existing.created_at
                || definition.last_run_at != existing.last_run_at
            {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            transaction
                .execute(
                    "UPDATE pod0_scheduled_tasks SET label=?1,prompt=?2,prompt_revision=?3,\
                 model_reference=?4,interval_ms=?5,next_run_at_ms=?6,task_revision=?7,\
                 updated_at_ms=?8 WHERE task_id=?9 AND active=1 AND task_revision=?10",
                    params![
                        definition.label,
                        definition.prompt,
                        definition.prompt_revision.into_bytes().as_slice(),
                        definition.model_reference,
                        to_i64(definition.interval_milliseconds)?,
                        definition.next_run_at.value(),
                        to_i64(definition.revision.value)?,
                        context.observed_at.value(),
                        definition.task_id.into_bytes().as_slice(),
                        to_i64(expected_revision.value)?,
                    ],
                )
                .map_err(|error| StorageError::sqlite("update scheduled task", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            finish_command(transaction, &context, Some(definition.task_id), None)?;
            Ok(ScheduledTaskMutationOutcome::Applied(definition))
        })
    }

    pub fn remove_task(
        &self,
        context: ScheduledAgentCommandContext,
        task_id: ScheduledTaskId,
        expected_revision: StateRevision,
    ) -> Result<ScheduledTaskRemovalOutcome, StorageError> {
        validate_context(&context)?;
        self.write(|transaction| {
            if let Some(receipt) = command_receipt(transaction, &context)? {
                let stored = receipt.task_id.ok_or(StorageError::ScheduledAgentCommandConflict)?;
                return Ok(ScheduledTaskRemovalOutcome::Duplicate {
                    task_id: stored,
                    revision: receipt.applied_revision,
                });
            }
            let existing = read_task(transaction, task_id, true)?
                .ok_or(StorageError::ScheduledAgentTaskNotFound)?;
            if existing.revision != expected_revision {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            let revision = StateRevision::new(expected_revision.value.saturating_add(1));
            transaction.execute(
                "UPDATE pod0_scheduled_tasks SET active=0,task_revision=?1,removed_at_ms=?2,\
                 updated_at_ms=?2 WHERE task_id=?3 AND active=1 AND task_revision=?4",
                params![
                    to_i64(revision.value)?, context.observed_at.value(),
                    task_id.into_bytes().as_slice(), to_i64(expected_revision.value)?,
                ],
            ).map_err(|error| StorageError::sqlite("remove scheduled task", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            transaction.execute(
                "UPDATE pod0_scheduled_occurrences SET stage='obsolete',\
                 workflow_revision=workflow_revision+1,failure_code=NULL,failure_wire_code=NULL,\
                 failure_detail=NULL,failure_retryable=0,updated_at_ms=?1 WHERE task_id=?2 \
                 AND stage IN('pending','requested','retry_scheduled','blocked')",
                params![context.observed_at.value(), task_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("obsolete removed scheduled runs", error))?;
            transaction.execute(
                "UPDATE pod0_scheduled_attempts SET state='cancelled',updated_at_ms=?1 \
                 WHERE occurrence_id IN(SELECT occurrence_id FROM pod0_scheduled_occurrences \
                 WHERE task_id=?2 AND stage='obsolete') AND state IN('requested','retry_scheduled','blocked')",
                params![context.observed_at.value(), task_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("retire removed scheduled attempts", error))?;
            finish_command(transaction, &context, Some(task_id), None)?;
            Ok(ScheduledTaskRemovalOutcome::Applied { task_id, revision })
        })
    }
}

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
    transaction
        .execute(
            "INSERT INTO pod0_scheduled_command_receipts(command_id,command_fingerprint,task_id,\
         occurrence_id,applied_revision,completed_at_ms) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                context.command_id.into_bytes().as_slice(),
                context.command_fingerprint.as_slice(),
                task_id.map(|value| value.into_bytes().to_vec()),
                occurrence_id.map(|value| value.into_bytes().to_vec()),
                next,
                context.observed_at.value(),
            ],
        )
        .map_err(|error| StorageError::sqlite("record scheduled command receipt", error))?;
    crate::scheduled_agent_store_codec::revision(next)
}

pub(crate) fn validate_context(context: &ScheduledAgentCommandContext) -> Result<(), StorageError> {
    if context.observed_at.value() < 0 {
        Err(StorageError::ScheduledAgentWorkflowConflict)
    } else {
        Ok(())
    }
}

fn insert_task(
    transaction: &Transaction<'_>,
    definition: &ScheduledTaskDefinition,
    now_ms: i64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_scheduled_tasks(task_id,label,prompt,prompt_revision,model_reference,\
         interval_ms,task_revision,last_run_at_ms,next_run_at_ms,active,created_at_ms,updated_at_ms,\
         removed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,?8,1,?9,?9,NULL)",
        params![
            definition.task_id.into_bytes().as_slice(), definition.label, definition.prompt,
            definition.prompt_revision.into_bytes().as_slice(), definition.model_reference,
            to_i64(definition.interval_milliseconds)?, to_i64(definition.revision.value)?,
            definition.next_run_at.value(), now_ms,
        ],
    ).map_err(|error| StorageError::sqlite("insert scheduled task", error))?;
    Ok(())
}

fn same_definition(left: &ScheduledTaskDefinition, right: &ScheduledTaskDefinition) -> bool {
    left == right
}

pub(crate) fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ScheduledAgentWorkflowConflict)
}
