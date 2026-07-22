use pod0_application::{
    ScheduledAgentOccurrenceState, ScheduledAgentTransition, cancel_scheduled_agent,
    retry_scheduled_agent,
};
use pod0_domain::{ScheduledOccurrenceId, StateRevision};
use rusqlite::params;

use crate::scheduled_agent_store_read::read_occurrence;
use crate::scheduled_agent_store_reconcile::persist_occurrence_state;
use crate::scheduled_agent_store_tasks::{command_receipt, finish_command, validate_context};
use crate::{ScheduledAgentCommandContext, ScheduledAgentStore, StorageError};

impl ScheduledAgentStore {
    pub fn cancel_occurrence(
        &self,
        context: ScheduledAgentCommandContext,
        occurrence_id: ScheduledOccurrenceId,
        expected_revision: StateRevision,
    ) -> Result<ScheduledAgentOccurrenceState, StorageError> {
        validate_context(&context)?;
        self.write(|transaction| {
            if let Some(receipt) = command_receipt(transaction, &context)? {
                let id = receipt
                    .occurrence_id
                    .ok_or(StorageError::ScheduledAgentCommandConflict)?;
                return read_occurrence(transaction, id)?
                    .ok_or(StorageError::ScheduledAgentWorkflowNotFound);
            }
            let previous = read_occurrence(transaction, occurrence_id)?
                .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
            if previous.revision != expected_revision {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            let mut next = previous.clone();
            match cancel_scheduled_agent(&mut next, context.observed_at) {
                ScheduledAgentTransition::Applied => {}
                ScheduledAgentTransition::IgnoredDuplicate => return Ok(previous),
                _ => return Err(StorageError::ScheduledAgentWorkflowConflict),
            }
            persist_occurrence_state(transaction, &previous, &next)?;
            if let Some(attempt_id) = next.attempt_id {
                transaction
                    .execute(
                        "UPDATE pod0_scheduled_attempts SET state='cancelled',\
                         failure_code='cancelled',failure_wire_code=NULL,failure_detail=NULL,\
                         failure_retryable=0,updated_at_ms=?1 WHERE attempt_id=?2 AND state NOT \
                         IN('succeeded','failed','cancelled','ambiguous')",
                        params![
                            context.observed_at.value(),
                            attempt_id.into_bytes().as_slice()
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("cancel scheduled attempt", error))?;
            }
            finish_command(
                transaction,
                &context,
                Some(next.task_id),
                Some(occurrence_id),
            )?;
            Ok(next)
        })
    }

    pub fn retry_occurrence(
        &self,
        context: ScheduledAgentCommandContext,
        occurrence_id: ScheduledOccurrenceId,
        expected_revision: StateRevision,
    ) -> Result<ScheduledAgentOccurrenceState, StorageError> {
        validate_context(&context)?;
        self.write(|transaction| {
            if let Some(receipt) = command_receipt(transaction, &context)? {
                let id = receipt
                    .occurrence_id
                    .ok_or(StorageError::ScheduledAgentCommandConflict)?;
                return read_occurrence(transaction, id)?
                    .ok_or(StorageError::ScheduledAgentWorkflowNotFound);
            }
            let previous = read_occurrence(transaction, occurrence_id)?
                .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
            if previous.revision != expected_revision {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            let mut next = previous.clone();
            if retry_scheduled_agent(&mut next, context.observed_at)
                != ScheduledAgentTransition::Applied
            {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            persist_occurrence_state(transaction, &previous, &next)?;
            finish_command(
                transaction,
                &context,
                Some(next.task_id),
                Some(occurrence_id),
            )?;
            Ok(next)
        })
    }
}
