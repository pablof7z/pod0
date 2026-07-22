use pod0_application::{ScheduledAgentOccurrenceState, advance_scheduled_task_after_completion};
use rusqlite::{Transaction, params};

use crate::scheduled_agent_store_observations::AttemptRow;
use crate::scheduled_agent_store_read::read_task;
use crate::scheduled_agent_store_tasks::to_i64;
use crate::{ScheduledAgentObservationInput, StorageError};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_completion(
    transaction: &Transaction<'_>,
    attempt: &AttemptRow,
    _previous: &ScheduledAgentOccurrenceState,
    next: &ScheduledAgentOccurrenceState,
    artifact_id: pod0_domain::GeneratedArtifactId,
    output_digest: pod0_domain::ContentDigest,
    output_excerpt: &str,
    input: &ScheduledAgentObservationInput,
    fingerprint: [u8; 32],
) -> Result<(), StorageError> {
    let definition = read_task(transaction, next.task_id, true)?
        .ok_or(StorageError::ScheduledAgentTaskNotFound)?;
    let advanced = advance_scheduled_task_after_completion(&definition, next, input.observed_at)
        .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?;
    transaction
        .execute(
            "INSERT INTO pod0_scheduled_completion_evidence(attempt_id,occurrence_id,request_id,\
         artifact_id,output_digest,output_excerpt,sequence_number,observation_fingerprint,\
         observed_at_ms,state,committed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'committed',?9)",
            params![
                attempt.attempt_id.into_bytes().as_slice(),
                next.occurrence_id.into_bytes().as_slice(),
                attempt.request_id.into_bytes().as_slice(),
                artifact_id.into_bytes().as_slice(),
                output_digest.into_bytes().as_slice(),
                output_excerpt,
                to_i64(input.sequence_number)?,
                fingerprint.as_slice(),
                input.observed_at.value(),
            ],
        )
        .map_err(|error| StorageError::sqlite("record scheduled completion evidence", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_generated_artifacts(artifact_id,occurrence_id,attempt_id,kind,\
         content_digest,bounded_excerpt,selected_at_ms) \
         VALUES(?1,?2,?3,'scheduled_agent_output',?4,?5,?6)",
            params![
                artifact_id.into_bytes().as_slice(),
                next.occurrence_id.into_bytes().as_slice(),
                attempt.attempt_id.into_bytes().as_slice(),
                output_digest.into_bytes().as_slice(),
                output_excerpt,
                input.observed_at.value(),
            ],
        )
        .map_err(|error| StorageError::sqlite("select generated artifact", error))?;
    transaction
        .execute(
            "UPDATE pod0_scheduled_attempts SET state='succeeded',last_sequence_number=?1,\
         last_observation_fingerprint=?2,failure_code=NULL,failure_wire_code=NULL,\
         failure_detail=NULL,failure_retryable=0,updated_at_ms=?3 WHERE attempt_id=?4",
            params![
                to_i64(input.sequence_number)?,
                fingerprint.as_slice(),
                input.observed_at.value(),
                attempt.attempt_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("commit scheduled attempt", error))?;
    transaction
        .execute(
            "UPDATE pod0_scheduled_tasks SET last_run_at_ms=?1,next_run_at_ms=?2,task_revision=?3,\
         updated_at_ms=?1 WHERE task_id=?4 AND active=1 AND task_revision=?5",
            params![
                input.observed_at.value(),
                advanced.next_run_at.value(),
                to_i64(advanced.revision.value)?,
                advanced.task_id.into_bytes().as_slice(),
                to_i64(definition.revision.value)?,
            ],
        )
        .map_err(|error| StorageError::sqlite("advance scheduled recurrence", error))?;
    if transaction.changes() != 1 {
        return Err(StorageError::ScheduledAgentWorkflowConflict);
    }
    Ok(())
}
