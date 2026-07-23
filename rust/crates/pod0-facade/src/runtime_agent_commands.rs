use pod0_application::{
    AgentExecutionKind, AgentTurnStage, AgentTurnStart, AgentTurnState, AgentWorkflowAcceptance,
    ApplicationCommand, CommandEnvelope, CoreFailureCode, OperationResult, agent_tool_policy,
};
use pod0_domain::{AgentTurnId, ConversationId, StateRevision, UnixTimestampMilliseconds};
use pod0_storage::{AgentAuditKind, AgentCommandContext, AgentTurnMutation, StorageError};

use crate::runtime_agent_modules::identity::{
    agent_command_id, agent_execution_fence_id, agent_fingerprint, agent_turn_id, model_fence_id,
};
use crate::runtime_agent_modules::persistence::persist_agent_update;
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

const MAX_ACTIVE_AGENT_TURNS: u16 = 32;

impl FacadeState {
    pub(crate) fn accept_agent_command(
        &mut self,
        envelope: &CommandEnvelope,
        command: ApplicationCommand,
        fingerprint: &str,
    ) {
        match command {
            ApplicationCommand::StartAgentTurn {
                conversation_id,
                user_input,
                model_reference,
                available_tools,
            } => self.start_agent_turn(
                envelope,
                fingerprint,
                conversation_id,
                user_input,
                model_reference,
                available_tools,
            ),
            ApplicationCommand::CancelAgentTurn {
                turn_id,
                expected_turn_revision,
            } => self.cancel_agent_turn(envelope, fingerprint, turn_id, expected_turn_revision),
            _ => unreachable!("agent dispatcher received another command"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn start_agent_turn(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        conversation_id: Option<ConversationId>,
        user_input: String,
        model_reference: String,
        available_tools: Vec<pod0_application::AgentToolName>,
    ) {
        let Some(store) = self.agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        match store.recoverable_turns(MAX_ACTIVE_AGENT_TURNS) {
            Ok(active) if active.len() >= usize::from(MAX_ACTIVE_AGENT_TURNS) => {
                self.fail(envelope.command_id, CoreFailureCode::HostUnavailable);
                return;
            }
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                return;
            }
            Ok(_) => {}
        }
        let now = self.now();
        let turn_id = agent_turn_id(envelope.command_id);
        let conversation_id =
            conversation_id.unwrap_or_else(|| ConversationId::from_bytes(turn_id.into_bytes()));
        let state = AgentTurnState::start(AgentTurnStart {
            conversation_id,
            turn_id,
            model_fence_id: model_fence_id(turn_id),
            user_input,
            model_reference,
            available_tools,
            cancellation_id: envelope.cancellation_id,
            observed_at: now,
        });
        let Ok(state) = state else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let context = AgentCommandContext {
            command_id: envelope.command_id,
            command_fingerprint: agent_fingerprint(
                b"pod0:agent-start-command:v1",
                &[fingerprint.as_bytes()],
            ),
            observed_at: now,
        };
        match store.start_turn(context, &state) {
            Ok(outcome) => {
                let persisted = outcome.state().clone();
                if persisted.projection().stage == AgentTurnStage::AwaitingModel {
                    let _ = self.queue_agent_model_request(envelope.command_id, &persisted);
                }
                self.succeed(
                    envelope.command_id,
                    Some(OperationResult::AgentTurnStarted {
                        conversation_id,
                        turn_id,
                    }),
                );
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    fn cancel_agent_turn(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        turn_id: AgentTurnId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let mut turn_cancellation_id = None;
        let result = store.turn(turn_id).and_then(|state| {
            let mut state = state.ok_or(StorageError::AgentTurnNotFound)?;
            turn_cancellation_id = Some(state.cancellation_id());
            if state.projection().revision != expected_revision
                || state.cancel(self.now()) != AgentWorkflowAcceptance::Updated
            {
                return Err(StorageError::AgentTurnConflict);
            }
            store.update_turn(
                AgentCommandContext {
                    command_id: envelope.command_id,
                    command_fingerprint: agent_fingerprint(
                        b"pod0:agent-cancel-command:v1",
                        &[fingerprint.as_bytes()],
                    ),
                    observed_at: self.now(),
                },
                AgentTurnMutation {
                    expected_revision,
                    audit_kind: AgentAuditKind::Cancelled,
                },
                &state,
            )
        });
        match result {
            Ok(_) => {
                self.retire_agent_recalls_for_turn(turn_id);
                if let Some(cancellation_id) = turn_cancellation_id {
                    self.cancel_operation(cancellation_id);
                }
                self.withdraw_agent_requests(turn_id);
                self.succeed(envelope.command_id, None);
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

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
                    let _ = state.mark_outcome_ambiguous(now);
                    self.persist_recovered_agent_state(&store, projection.revision, &state, now)?;
                }
                AgentTurnStage::ApprovalRequired => {
                    let command_id = agent_command_id(b"approval-recovery", projection.turn_id);
                    let _ = self.queue_agent_approval_request(command_id, &state);
                }
                AgentTurnStage::Authorized => {
                    self.begin_authorized_agent_action(&store, state, now)?;
                }
                AgentTurnStage::Executing => {
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

    pub(super) fn begin_authorized_agent_action(
        &mut self,
        store: &pod0_storage::AgentStore,
        mut state: AgentTurnState,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<(), StorageError> {
        let before = state.projection();
        let proposal = before
            .proposal
            .as_ref()
            .ok_or(StorageError::InvalidAgentState)?;
        let fence = agent_execution_fence_id(proposal.proposal_id, proposal.proposal_digest);
        if state.begin_execution(fence, observed_at) != AgentWorkflowAcceptance::Updated {
            return Err(StorageError::AgentTurnConflict);
        }
        let command_id = agent_command_id(b"begin-execution", before.turn_id);
        let state = persist_agent_update(
            store,
            command_id,
            b"pod0:agent-begin-execution:v1",
            AgentAuditKind::ExecutionStarted,
            before.revision,
            state,
            observed_at,
        )?;
        let execution = state
            .projection()
            .proposal
            .as_ref()
            .map(|proposal| agent_tool_policy(proposal.action.tool()).execution)
            .ok_or(StorageError::InvalidAgentState)?;
        match execution {
            AgentExecutionKind::RustCommit | AgentExecutionKind::RustProjection => {
                self.execute_internal_agent_action(store, state, observed_at)
            }
            AgentExecutionKind::NativeCapability
            | AgentExecutionKind::NativeConversationPresentation
            | AgentExecutionKind::NativeCapabilityAndNmpPublication => {
                let _ = self.queue_agent_capability_request(command_id, &state);
                Ok(())
            }
        }
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
