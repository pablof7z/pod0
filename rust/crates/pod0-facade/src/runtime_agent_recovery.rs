use pod0_application::{
    AgentCapabilityExecutionMode, AgentExecutionKind, AgentToolAction, AgentTurnStage,
    AgentTurnState, agent_tool_policy,
};
use pod0_domain::{StateRevision, UnixTimestampMilliseconds};
use pod0_storage::{AgentAuditKind, StorageError};

use crate::runtime_agent_modules::commands::MAX_ACTIVE_AGENT_TURNS;
use crate::runtime_agent_modules::identity::agent_command_id;
use crate::runtime_agent_modules::persistence::persist_agent_update;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn rehydrate_agent_turns(&mut self) -> Result<(), StorageError> {
        let Some(store) = self.agent_store.clone() else {
            return Ok(());
        };
        let now = self.now();
        let states = store.recoverable_turns(MAX_ACTIVE_AGENT_TURNS)?;
        for mut state in states {
            let projection = state.projection();
            match projection.stage {
                AgentTurnStage::AwaitingModel => {
                    if projection.commit.is_some() {
                        let command_id =
                            agent_command_id(b"continuation-recovery", projection.turn_id);
                        let _ = self.queue_agent_model_request(command_id, &state);
                    } else {
                        let _ = state.mark_outcome_ambiguous(now);
                        self.persist_recovered_agent_state(
                            &store,
                            projection.revision,
                            &state,
                            now,
                        )?;
                    }
                }
                AgentTurnStage::ApprovalRequired => {
                    let command_id = agent_command_id(b"approval-recovery", projection.turn_id);
                    let _ = self.queue_agent_approval_request(command_id, &state);
                }
                AgentTurnStage::Authorized => {
                    self.begin_authorized_agent_action(&store, state, now)?;
                }
                AgentTurnStage::Executing => {
                    if matches!(
                        projection
                            .proposal
                            .as_ref()
                            .map(|proposal| &proposal.action),
                        Some(AgentToolAction::GenerateTtsEpisode { .. })
                    ) {
                        let command_id =
                            agent_command_id(b"generated-audio-recovery", projection.turn_id);
                        let _ = self.queue_agent_capability_request(
                            command_id,
                            &state,
                            AgentCapabilityExecutionMode::RecoverExisting,
                        );
                        continue;
                    }
                    let execution = projection
                        .proposal
                        .as_ref()
                        .map(|proposal| agent_tool_policy(proposal.action.tool()).execution);
                    if matches!(
                        execution,
                        Some(AgentExecutionKind::RustCommit | AgentExecutionKind::RustProjection)
                    ) {
                        self.execute_internal_agent_action(&store, state, now)?;
                    } else {
                        let _ = state.mark_outcome_ambiguous(now);
                        self.persist_recovered_agent_state(
                            &store,
                            projection.revision,
                            &state,
                            now,
                        )?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn persist_recovered_agent_state(
        &mut self,
        store: &pod0_storage::AgentStore,
        expected_revision: StateRevision,
        state: &AgentTurnState,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<(), StorageError> {
        let turn_id = state.projection().turn_id;
        let command_id = agent_command_id(b"recovery", turn_id);
        let _ = persist_agent_update(
            store,
            command_id,
            b"pod0:agent-recovery:v1",
            AgentAuditKind::Recovered,
            expected_revision,
            state.clone(),
            observed_at,
        )?;
        Ok(())
    }
}
