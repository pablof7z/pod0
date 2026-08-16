use pod0_application::AgentTurnState;
use pod0_domain::AgentTurnId;
use rusqlite::params;

use crate::{
    AgentApprovalObservationCommitInput, AgentApprovalObservationCommitOutcome,
    AgentCommandContext, AgentModelObservationCommitInput, AgentModelObservationCommitOutcome,
    AgentMutationOutcome, AgentStore, StorageError,
};

impl AgentStore {
    pub fn start_turn_activity(
        &self,
        context: AgentCommandContext,
        state: &AgentTurnState,
    ) -> Result<AgentMutationOutcome, StorageError> {
        crate::transition_commit::commit_agent_turn_start(self.path(), context, state)
    }

    pub fn commit_model_observation(
        &self,
        input: AgentModelObservationCommitInput,
    ) -> Result<AgentModelObservationCommitOutcome, StorageError> {
        crate::transition_commit::commit_agent_model_observation(self.path(), input)
    }

    pub fn commit_approval_observation(
        &self,
        input: AgentApprovalObservationCommitInput,
    ) -> Result<AgentApprovalObservationCommitOutcome, StorageError> {
        crate::transition_commit::commit_agent_approval_observation(self.path(), input)
    }

    pub fn commit_capability_observation(
        &self,
        input: crate::AgentCapabilityObservationCommitInput,
    ) -> Result<crate::AgentCapabilityObservationCommitOutcome, StorageError> {
        crate::transition_commit::commit_agent_capability_observation(self.path(), input)
    }

    pub fn commit_recall_observation(
        &self,
        input: crate::AgentRecallObservationCommitInput,
    ) -> Result<crate::AgentRecallObservationCommitOutcome, StorageError> {
        crate::transition_commit::commit_agent_recall_observation(self.path(), input)
    }

    pub fn cancel_turn_activity(
        &self,
        context: AgentCommandContext,
        turn_id: AgentTurnId,
        expected_revision: pod0_domain::StateRevision,
    ) -> Result<crate::AgentCancellationCommitOutcome, StorageError> {
        crate::transition_commit::commit_agent_cancellation(
            self.path(),
            context,
            turn_id,
            expected_revision,
        )
    }

    pub fn begin_execution_from_internal_command(
        &self,
        command: crate::PendingInternalCommand,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<AgentTurnState, StorageError> {
        crate::transition_commit::commit_agent_execution(self.path(), command, observed_at)
    }

    pub fn commit_projection_result(
        &self,
        command: crate::PendingInternalCommand,
        result: Result<String, String>,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<AgentTurnState, StorageError> {
        crate::transition_commit::commit_agent_projection_result(
            self.path(),
            command,
            result,
            observed_at,
        )
    }

    pub fn commit_agent_note(
        &self,
        command: crate::PendingInternalCommand,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<pod0_domain::NoteId, StorageError> {
        crate::transition_commit::commit_agent_note(self.path(), command, observed_at)
    }

    pub fn commit_agent_memory(
        &self,
        command: crate::PendingInternalCommand,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<pod0_domain::MemoryId, StorageError> {
        crate::transition_commit::commit_agent_memory(self.path(), command, observed_at)
    }

    pub fn commit_agent_clip(
        &self,
        command: crate::PendingInternalCommand,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<pod0_domain::ClipId, StorageError> {
        crate::transition_commit::commit_agent_clip(self.path(), command, observed_at)
    }

    pub fn commit_agent_category(
        &self,
        command: crate::PendingInternalCommand,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<pod0_domain::CategoryId, StorageError> {
        crate::transition_commit::commit_agent_category(self.path(), command, observed_at)
    }

    pub fn commit_tool_completion(
        &self,
        command: crate::PendingInternalCommand,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<AgentTurnState, StorageError> {
        crate::transition_commit::commit_agent_tool_completion(self.path(), command, observed_at)
    }

    pub fn has_open_model_effect(&self, turn_id: AgentTurnId) -> Result<bool, StorageError> {
        self.has_open_effect(turn_id, 8, "read open agent model effect")
    }

    pub fn has_open_approval_effect(&self, turn_id: AgentTurnId) -> Result<bool, StorageError> {
        self.has_open_effect(turn_id, 9, "read open agent approval effect")
    }

    pub fn has_open_capability_effect(&self, turn_id: AgentTurnId) -> Result<bool, StorageError> {
        self.has_open_effect(turn_id, 10, "read open agent capability effect")
    }

    fn has_open_effect(
        &self,
        turn_id: AgentTurnId,
        kind_code: u8,
        operation: &'static str,
    ) -> Result<bool, StorageError> {
        self.read(|connection| {
            connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pod0_effect_intents WHERE effect_kind_code=?1 \
                     AND subject_code=4 AND subject_id=?2 AND state_code IN (1,2))",
                    params![kind_code, turn_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .map_err(|error| StorageError::sqlite(operation, error))
        })
    }
}
