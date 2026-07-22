use pod0_application::{ScheduledAgentTransition, mark_scheduled_agent_ambiguous_after_restart};
use pod0_domain::ScheduledOccurrenceId;
use rusqlite::{Connection, params};

use crate::scheduled_agent_store_read::{pending_requests, read_occurrence};
use crate::scheduled_agent_store_reconcile::persist_occurrence_state;
use crate::{ScheduledAgentRecoveryReport, ScheduledAgentStore, StorageError};

impl ScheduledAgentStore {
    pub fn recover_after_restart(
        &self,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<ScheduledAgentRecoveryReport, StorageError> {
        if observed_at.value() < 0 {
            return Err(StorageError::ScheduledAgentWorkflowConflict);
        }
        self.write(|transaction| {
            let occurrence_ids = accepted_occurrences(transaction)?;
            let mut ambiguous_occurrences = Vec::with_capacity(occurrence_ids.len());
            for occurrence_id in occurrence_ids {
                let previous = read_occurrence(transaction, occurrence_id)?
                    .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
                let mut next = previous.clone();
                match mark_scheduled_agent_ambiguous_after_restart(&mut next, observed_at) {
                    ScheduledAgentTransition::Applied => {}
                    ScheduledAgentTransition::IgnoredDuplicate => continue,
                    _ => return Err(StorageError::ScheduledAgentWorkflowConflict),
                }
                persist_occurrence_state(transaction, &previous, &next)?;
                transaction.execute(
                    "UPDATE pod0_scheduled_attempts SET state='ambiguous',\
                     failure_code='unsafe_to_retry',failure_wire_code=NULL,\
                     failure_detail='Provider acceptance was persisted before process termination.',\
                     failure_retryable=0,updated_at_ms=?1 WHERE attempt_id=?2 AND state='host_accepted'",
                    params![
                        observed_at.value(),
                        next.attempt_id
                            .ok_or(StorageError::ScheduledAgentWorkflowConflict)?
                            .into_bytes()
                            .as_slice(),
                    ],
                ).map_err(|error| StorageError::sqlite("fence ambiguous scheduled attempt", error))?;
                if transaction.changes() != 1 {
                    return Err(StorageError::ScheduledAgentWorkflowConflict);
                }
                ambiguous_occurrences.push(occurrence_id);
            }
            Ok(ScheduledAgentRecoveryReport {
                reissued_requests: pending_requests(transaction, None, u16::MAX)?,
                ambiguous_occurrences,
            })
        })
    }
}

fn accepted_occurrences(
    connection: &Connection,
) -> Result<Vec<ScheduledOccurrenceId>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT occurrence_id FROM pod0_scheduled_occurrences WHERE stage='host_accepted' \
             ORDER BY updated_at_ms,occurrence_id",
        )
        .map_err(|error| StorageError::sqlite("prepare accepted scheduled runs", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| StorageError::sqlite("query accepted scheduled runs", error))?;
    rows.map(|row| {
        let bytes =
            row.map_err(|error| StorageError::sqlite("read accepted scheduled run", error))?;
        crate::scheduled_agent_store_codec::occurrence_id(&bytes)
    })
    .collect()
}
