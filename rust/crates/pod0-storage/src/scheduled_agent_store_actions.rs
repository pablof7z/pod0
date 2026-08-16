use crate::scheduled_agent_store_tasks::validate_context;
use crate::{ScheduledAgentCommandContext, ScheduledAgentStore, StorageError};
use pod0_application::ScheduledAgentOccurrenceState;
use pod0_domain::{ScheduledOccurrenceId, StateRevision};

impl ScheduledAgentStore {
    pub fn cancel_occurrence(
        &self,
        context: ScheduledAgentCommandContext,
        occurrence_id: ScheduledOccurrenceId,
        expected_revision: StateRevision,
    ) -> Result<ScheduledAgentOccurrenceState, StorageError> {
        validate_context(&context)?;
        crate::transition_commit::commit_scheduled_occurrence_cancel(
            self.path(),
            context,
            occurrence_id,
            expected_revision,
        )
    }

    pub fn retry_occurrence(
        &self,
        context: ScheduledAgentCommandContext,
        occurrence_id: ScheduledOccurrenceId,
        expected_revision: StateRevision,
    ) -> Result<ScheduledAgentOccurrenceState, StorageError> {
        validate_context(&context)?;
        crate::transition_commit::commit_scheduled_occurrence_retry(
            self.path(),
            context,
            occurrence_id,
            expected_revision,
        )
    }
}
