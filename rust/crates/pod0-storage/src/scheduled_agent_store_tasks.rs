use pod0_application::ScheduledTaskDefinition;
use pod0_domain::{ScheduledTaskId, StateRevision};
use rusqlite::{Transaction, params};

use crate::scheduled_agent_store_read::read_task;
use crate::{
    ScheduledAgentCommandContext, ScheduledAgentStore, ScheduledTaskMutationOutcome,
    ScheduledTaskRemovalOutcome, StorageError,
};

pub(crate) use crate::scheduled_agent_store_command_receipts::{
    advance_core_revision, command_receipt, core_revision, finish_command, to_i64, validate_context,
};

impl ScheduledAgentStore {
    pub fn ensure_task(
        &self,
        context: ScheduledAgentCommandContext,
        definition: ScheduledTaskDefinition,
    ) -> Result<ScheduledTaskMutationOutcome, StorageError> {
        crate::transition_commit::commit_scheduled_task_ensure(self.path(), context, definition)
    }

    pub fn update_task(
        &self,
        context: ScheduledAgentCommandContext,
        expected_revision: StateRevision,
        definition: ScheduledTaskDefinition,
    ) -> Result<ScheduledTaskMutationOutcome, StorageError> {
        crate::transition_commit::commit_scheduled_task_update(
            self.path(),
            context,
            expected_revision,
            definition,
        )
    }

    pub fn remove_task(
        &self,
        context: ScheduledAgentCommandContext,
        task_id: ScheduledTaskId,
        expected_revision: StateRevision,
    ) -> Result<ScheduledTaskRemovalOutcome, StorageError> {
        crate::transition_commit::commit_scheduled_task_remove(
            self.path(),
            context,
            task_id,
            expected_revision,
        )
    }
}

pub(crate) fn update_task_in_transaction(
    transaction: &Transaction<'_>,
    context: &ScheduledAgentCommandContext,
    expected_revision: StateRevision,
    definition: ScheduledTaskDefinition,
) -> Result<ScheduledTaskMutationOutcome, StorageError> {
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
}

pub(crate) fn ensure_task_in_transaction(
    transaction: &Transaction<'_>,
    context: &ScheduledAgentCommandContext,
    definition: ScheduledTaskDefinition,
) -> Result<ScheduledTaskMutationOutcome, StorageError> {
    if let Some(receipt) = command_receipt(transaction, context)? {
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
        finish_command(transaction, context, Some(definition.task_id), None)?;
        return Ok(ScheduledTaskMutationOutcome::Applied(existing));
    }
    insert_task(transaction, &definition, context.observed_at.value())?;
    finish_command(transaction, context, Some(definition.task_id), None)?;
    Ok(ScheduledTaskMutationOutcome::Applied(definition))
}

pub(crate) fn remove_task_in_transaction(
    transaction: &Transaction<'_>,
    context: &ScheduledAgentCommandContext,
    task_id: ScheduledTaskId,
    expected_revision: StateRevision,
) -> Result<ScheduledTaskRemovalOutcome, StorageError> {
    if let Some(receipt) = command_receipt(transaction, &context)? {
        let stored = receipt
            .task_id
            .ok_or(StorageError::ScheduledAgentCommandConflict)?;
        return Ok(ScheduledTaskRemovalOutcome::Duplicate {
            task_id: stored,
            revision: receipt.applied_revision,
        });
    }
    let existing =
        read_task(transaction, task_id, true)?.ok_or(StorageError::ScheduledAgentTaskNotFound)?;
    if existing.revision != expected_revision {
        return Err(StorageError::ScheduledAgentWorkflowConflict);
    }
    let revision = StateRevision::new(expected_revision.value.saturating_add(1));
    transaction
        .execute(
            "UPDATE pod0_scheduled_tasks SET active=0,task_revision=?1,removed_at_ms=?2,\
                 updated_at_ms=?2 WHERE task_id=?3 AND active=1 AND task_revision=?4",
            params![
                to_i64(revision.value)?,
                context.observed_at.value(),
                task_id.into_bytes().as_slice(),
                to_i64(expected_revision.value)?,
            ],
        )
        .map_err(|error| StorageError::sqlite("remove scheduled task", error))?;
    if transaction.changes() != 1 {
        return Err(StorageError::ScheduledAgentWorkflowConflict);
    }
    transaction
        .execute(
            "UPDATE pod0_scheduled_occurrences SET stage='obsolete',\
                 workflow_revision=workflow_revision+1,failure_code=NULL,failure_wire_code=NULL,\
                 failure_detail=NULL,failure_retryable=0,updated_at_ms=?1 WHERE task_id=?2 \
                 AND stage IN('pending','requested','host_accepted','retry_scheduled','blocked')",
            params![context.observed_at.value(), task_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("obsolete removed scheduled runs", error))?;
    transaction
        .execute(
            "UPDATE pod0_scheduled_attempts SET state='cancelled',updated_at_ms=?1 \
                 WHERE occurrence_id IN(SELECT occurrence_id FROM pod0_scheduled_occurrences \
                 WHERE task_id=?2 AND stage='obsolete') \
                 AND state IN('requested','host_accepted','retry_scheduled','blocked')",
            params![context.observed_at.value(), task_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("retire removed scheduled attempts", error))?;
    finish_command(transaction, &context, Some(task_id), None)?;
    Ok(ScheduledTaskRemovalOutcome::Applied { task_id, revision })
}

pub(crate) fn insert_task(
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

pub(crate) fn same_definition(
    left: &ScheduledTaskDefinition,
    right: &ScheduledTaskDefinition,
) -> bool {
    left == right
}
